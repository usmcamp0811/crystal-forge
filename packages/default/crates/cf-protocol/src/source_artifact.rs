//! Validates and extracts canonical verified-source artifacts.
//!
//! Contract version 1 uses an uncompressed POSIX tar produced once by the
//! authoritative server. Both evaluators consume the same bytes. Extraction is
//! bounded and does not invoke an external archive program.

use std::collections::HashSet;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};

/// Current canonical source artifact format.
pub const VERIFIED_SOURCE_ARTIFACT_FORMAT_VERSION: u32 = 1;
/// Maximum accepted canonical source artifact size: 256 MiB.
pub const VERIFIED_SOURCE_ARTIFACT_MAX_BYTES: u64 = 256 * 1024 * 1024;
/// Maximum accepted expanded tracked-tree size: 1 GiB.
pub const VERIFIED_SOURCE_ARTIFACT_MAX_EXPANDED_BYTES: u64 = 1024 * 1024 * 1024;
/// Maximum number of tracked-tree entries in one artifact.
pub const VERIFIED_SOURCE_ARTIFACT_MAX_ENTRIES: usize = 100_000;

/// Describes why a canonical source artifact could not be extracted.
#[derive(Debug)]
pub enum SourceArtifactError {
    /// The artifact violates the version-1 format or safety limits.
    Invalid(String),
    /// A local filesystem read or write failed.
    Io(io::Error),
}

impl std::fmt::Display for SourceArtifactError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Invalid(message) => write!(formatter, "invalid source artifact: {message}"),
            Self::Io(error) => write!(formatter, "source artifact I/O failed: {error}"),
        }
    }
}

impl std::error::Error for SourceArtifactError {}

impl From<io::Error> for SourceArtifactError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

#[derive(Debug)]
struct PendingSymlink {
    path: PathBuf,
    target: PathBuf,
}

fn validated_path(path: &Path) -> Result<PathBuf, SourceArtifactError> {
    if path.as_os_str().is_empty() || path.as_os_str().len() > 4096 {
        return Err(SourceArtifactError::Invalid(
            "entry path is empty or too long".to_string(),
        ));
    }
    if path
        .components()
        .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(SourceArtifactError::Invalid(format!(
            "entry path is not relative and normalized: {}",
            path.display()
        )));
    }
    Ok(path.to_path_buf())
}

fn validated_symlink_target(path: &Path, target: &Path) -> Result<PathBuf, SourceArtifactError> {
    if target.as_os_str().is_empty() || target.is_absolute() {
        return Err(SourceArtifactError::Invalid(format!(
            "symlink {} has an empty or absolute target",
            path.display()
        )));
    }
    let mut depth = path
        .parent()
        .map_or(0, |parent| parent.components().count());
    for component in target.components() {
        match component {
            Component::Normal(_) => depth += 1,
            Component::CurDir => {}
            Component::ParentDir if depth > 0 => depth -= 1,
            Component::ParentDir => {
                return Err(SourceArtifactError::Invalid(format!(
                    "symlink {} escapes the source root",
                    path.display()
                )));
            }
            Component::RootDir | Component::Prefix(_) => {
                return Err(SourceArtifactError::Invalid(format!(
                    "symlink {} has an absolute target",
                    path.display()
                )));
            }
        }
    }
    Ok(target.to_path_buf())
}

fn has_symlink_ancestor(path: &Path, symlinks: &HashSet<PathBuf>) -> bool {
    path.ancestors()
        .skip(1)
        .any(|ancestor| symlinks.contains(ancestor))
}

fn validate_extraction_limits(
    entry_count: usize,
    expanded_bytes: u64,
) -> Result<(), SourceArtifactError> {
    if entry_count > VERIFIED_SOURCE_ARTIFACT_MAX_ENTRIES {
        return Err(SourceArtifactError::Invalid(format!(
            "artifact has more than {} entries",
            VERIFIED_SOURCE_ARTIFACT_MAX_ENTRIES
        )));
    }
    if expanded_bytes > VERIFIED_SOURCE_ARTIFACT_MAX_EXPANDED_BYTES {
        return Err(SourceArtifactError::Invalid(format!(
            "expanded artifact exceeds {} bytes",
            VERIFIED_SOURCE_ARTIFACT_MAX_EXPANDED_BYTES
        )));
    }
    Ok(())
}

/// Extracts one version-1 canonical source artifact into an empty directory.
///
/// The extractor accepts Git's global PAX metadata header, directories, regular
/// files, executable files, and relative symlinks that remain inside the source
/// root. It rejects absolute or non-normal entry paths, duplicate entries, hard
/// links, devices, FIFOs, escaping symlinks, symlink ancestors, and configured
/// size-limit violations.
/// The destination MUST exist and MUST be empty.
///
/// # Errors
///
/// Returns [`SourceArtifactError::Invalid`] for a malformed or unsafe artifact.
/// Returns [`SourceArtifactError::Io`] when local filesystem access fails.
pub fn extract_verified_source_artifact(
    artifact_path: &Path,
    destination: &Path,
) -> Result<(), SourceArtifactError> {
    let metadata = fs::metadata(artifact_path)?;
    if !metadata.is_file() || metadata.len() > VERIFIED_SOURCE_ARTIFACT_MAX_BYTES {
        return Err(SourceArtifactError::Invalid(format!(
            "artifact size exceeds {} bytes",
            VERIFIED_SOURCE_ARTIFACT_MAX_BYTES
        )));
    }
    if fs::read_dir(destination)?.next().is_some() {
        return Err(SourceArtifactError::Invalid(
            "destination is not empty".to_string(),
        ));
    }

    let file = File::open(artifact_path)?;
    let mut archive = tar::Archive::new(file);
    let mut paths = HashSet::new();
    let mut symlink_paths = HashSet::new();
    let mut pending_symlinks = Vec::new();
    let mut expanded_bytes = 0_u64;
    let mut entry_count = 0_usize;

    for entry in archive.entries()? {
        let mut entry = entry?;
        entry_count = entry_count.saturating_add(1);
        validate_extraction_limits(entry_count, expanded_bytes)?;

        let entry_type = entry.header().entry_type();
        // COMPATIBILITY: `git archive --format=tar` emits a global PAX header
        // that records the source commit. The tar crate returns this metadata
        // as an entry but does not apply it to later paths or file contents.
        if entry_type.is_pax_global_extensions() {
            continue;
        }

        let path = validated_path(&entry.path()?)?;
        if !paths.insert(path.clone()) {
            return Err(SourceArtifactError::Invalid(format!(
                "artifact contains duplicate entry {}",
                path.display()
            )));
        }
        let output_path = destination.join(&path);

        if entry_type.is_dir() {
            fs::create_dir_all(&output_path)?;
            continue;
        }
        if entry_type.is_symlink() {
            let target = entry
                .link_name()?
                .ok_or_else(|| {
                    SourceArtifactError::Invalid(format!(
                        "symlink {} has no target",
                        path.display()
                    ))
                })?
                .into_owned();
            let target = validated_symlink_target(&path, &target)?;
            symlink_paths.insert(path.clone());
            pending_symlinks.push(PendingSymlink { path, target });
            continue;
        }
        if !entry_type.is_file() {
            return Err(SourceArtifactError::Invalid(format!(
                "entry {} has unsupported type",
                path.display()
            )));
        }

        let size = entry.header().size()?;
        expanded_bytes = expanded_bytes.checked_add(size).ok_or_else(|| {
            SourceArtifactError::Invalid("expanded artifact size overflowed".to_string())
        })?;
        validate_extraction_limits(entry_count, expanded_bytes)?;
        if let Some(parent) = output_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&output_path)?;
        let copied = io::copy(&mut entry.by_ref().take(size), &mut output)?;
        if copied != size {
            return Err(SourceArtifactError::Invalid(format!(
                "entry {} ended before its declared size",
                path.display()
            )));
        }
        output.flush()?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let archive_mode = entry.header().mode()?;
            let mode = if archive_mode & 0o111 == 0 {
                0o644
            } else {
                0o755
            };
            fs::set_permissions(&output_path, fs::Permissions::from_mode(mode))?;
        }
    }

    if paths
        .iter()
        .any(|path| has_symlink_ancestor(path, &symlink_paths))
    {
        return Err(SourceArtifactError::Invalid(
            "artifact contains an entry below a symlink".to_string(),
        ));
    }
    for symlink in pending_symlinks {
        let output_path = destination.join(&symlink.path);
        if let Some(parent) = output_path.parent() {
            fs::create_dir_all(parent)?;
        }
        #[cfg(unix)]
        std::os::unix::fs::symlink(&symlink.target, &output_path)?;
        #[cfg(not(unix))]
        return Err(SourceArtifactError::Invalid(
            "version-1 source artifacts require Unix symlink support".to_string(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn append_file(builder: &mut tar::Builder<Vec<u8>>, path: &str, mode: u32, bytes: &[u8]) {
        let mut header = tar::Header::new_ustar();
        header.set_size(bytes.len() as u64);
        header.set_mode(mode);
        header.set_entry_type(tar::EntryType::Regular);
        header.set_cksum();
        builder
            .append_data(&mut header, path, Cursor::new(bytes))
            .expect("test entry should append");
    }

    #[test]
    fn extraction_accepts_git_global_pax_metadata() {
        let commit = b"52 comment=169fa07f128d235bef0aeae239783c4a02abb013\n";
        let mut builder = tar::Builder::new(Vec::new());
        let mut header = tar::Header::new_ustar();
        header.set_size(commit.len() as u64);
        header.set_mode(0o666);
        header.set_entry_type(tar::EntryType::XGlobalHeader);
        header.set_cksum();
        builder
            .append_data(
                &mut header,
                "pax_global_header",
                Cursor::new(commit.as_slice()),
            )
            .expect("Git PAX metadata should append");
        append_file(&mut builder, "flake.lock", 0o644, b"{}\n");
        let bytes = builder.into_inner().expect("test archive should finish");

        let temporary = tempfile::tempdir().expect("temporary directory should exist");
        let artifact = temporary.path().join("source.tar");
        fs::write(&artifact, bytes).expect("test archive should write");
        let output = temporary.path().join("tree");
        fs::create_dir(&output).expect("output should exist");

        extract_verified_source_artifact(&artifact, &output)
            .expect("Git archive metadata should be ignored safely");
        assert_eq!(
            fs::read(output.join("flake.lock")).expect("flake.lock should extract"),
            b"{}\n"
        );
        assert!(!output.join("pax_global_header").exists());
    }

    #[cfg(unix)]
    #[test]
    fn extraction_preserves_executable_files_and_internal_symlinks() {
        use std::os::unix::fs::PermissionsExt;

        let mut builder = tar::Builder::new(Vec::new());
        append_file(&mut builder, "bin/run", 0o755, b"#!/bin/sh\n");
        let mut link = tar::Header::new_ustar();
        link.set_size(0);
        link.set_mode(0o777);
        link.set_entry_type(tar::EntryType::Symlink);
        link.set_link_name("bin/run")
            .expect("link target should fit");
        link.set_cksum();
        builder
            .append_data(&mut link, "run", io::empty())
            .expect("test symlink should append");
        let bytes = builder.into_inner().expect("test archive should finish");

        let temporary = tempfile::tempdir().expect("temporary directory should exist");
        let artifact = temporary.path().join("source.tar");
        fs::write(&artifact, bytes).expect("test archive should write");
        let output = temporary.path().join("tree");
        fs::create_dir(&output).expect("output should exist");
        extract_verified_source_artifact(&artifact, &output).expect("artifact should extract");

        assert_ne!(
            fs::metadata(output.join("bin/run"))
                .expect("file should exist")
                .permissions()
                .mode()
                & 0o111,
            0
        );
        assert_eq!(
            fs::read_link(output.join("run")).expect("symlink should exist"),
            PathBuf::from("bin/run")
        );
    }

    #[test]
    fn extraction_rejects_duplicate_paths() {
        let mut builder = tar::Builder::new(Vec::new());
        append_file(&mut builder, "duplicate", 0o644, b"first");
        append_file(&mut builder, "duplicate", 0o644, b"second");
        let bytes = builder.into_inner().expect("test archive should finish");
        let temporary = tempfile::tempdir().expect("temporary directory should exist");
        let artifact = temporary.path().join("source.tar");
        fs::write(&artifact, bytes).expect("test archive should write");
        let output = temporary.path().join("tree");
        fs::create_dir(&output).expect("output should exist");

        assert!(matches!(
            extract_verified_source_artifact(&artifact, &output),
            Err(SourceArtifactError::Invalid(message)) if message.contains("duplicate")
        ));
    }

    #[test]
    fn extraction_rejects_escaping_symlinks() {
        let mut builder = tar::Builder::new(Vec::new());
        let mut link = tar::Header::new_ustar();
        link.set_size(0);
        link.set_mode(0o777);
        link.set_entry_type(tar::EntryType::Symlink);
        link.set_link_name("../outside")
            .expect("link target should fit");
        link.set_cksum();
        builder
            .append_data(&mut link, "escape", io::empty())
            .expect("test symlink should append");
        let bytes = builder.into_inner().expect("test archive should finish");
        let temporary = tempfile::tempdir().expect("temporary directory should exist");
        let artifact = temporary.path().join("source.tar");
        fs::write(&artifact, bytes).expect("test archive should write");
        let output = temporary.path().join("tree");
        fs::create_dir(&output).expect("output should exist");

        assert!(matches!(
            extract_verified_source_artifact(&artifact, &output),
            Err(SourceArtifactError::Invalid(message)) if message.contains("escapes")
        ));
    }

    #[test]
    fn extraction_rejects_hard_links() {
        let mut builder = tar::Builder::new(Vec::new());
        append_file(&mut builder, "target", 0o644, b"data");
        let mut link = tar::Header::new_ustar();
        link.set_size(0);
        link.set_mode(0o644);
        link.set_entry_type(tar::EntryType::Link);
        link.set_link_name("target")
            .expect("link target should fit");
        link.set_cksum();
        builder
            .append_data(&mut link, "hard-link", io::empty())
            .expect("test hard link should append");
        let bytes = builder.into_inner().expect("test archive should finish");
        let temporary = tempfile::tempdir().expect("temporary directory should exist");
        let artifact = temporary.path().join("source.tar");
        fs::write(&artifact, bytes).expect("test archive should write");
        let output = temporary.path().join("tree");
        fs::create_dir(&output).expect("output should exist");

        assert!(matches!(
            extract_verified_source_artifact(&artifact, &output),
            Err(SourceArtifactError::Invalid(message)) if message.contains("unsupported type")
        ));
    }

    #[test]
    fn extraction_rejects_entries_below_symlinks() {
        let mut builder = tar::Builder::new(Vec::new());
        let mut link = tar::Header::new_ustar();
        link.set_size(0);
        link.set_mode(0o777);
        link.set_entry_type(tar::EntryType::Symlink);
        link.set_link_name("real").expect("link target should fit");
        link.set_cksum();
        builder
            .append_data(&mut link, "linked", io::empty())
            .expect("test symlink should append");
        append_file(&mut builder, "linked/file", 0o644, b"data");
        let bytes = builder.into_inner().expect("test archive should finish");
        let temporary = tempfile::tempdir().expect("temporary directory should exist");
        let artifact = temporary.path().join("source.tar");
        fs::write(&artifact, bytes).expect("test archive should write");
        let output = temporary.path().join("tree");
        fs::create_dir(&output).expect("output should exist");

        assert!(matches!(
            extract_verified_source_artifact(&artifact, &output),
            Err(SourceArtifactError::Invalid(message)) if message.contains("below a symlink")
        ));
    }

    #[test]
    fn extraction_rejects_absolute_paths_and_limit_overruns() {
        assert!(validated_path(Path::new("/absolute")).is_err());
        assert!(validate_extraction_limits(VERIFIED_SOURCE_ARTIFACT_MAX_ENTRIES + 1, 0).is_err());
        assert!(
            validate_extraction_limits(1, VERIFIED_SOURCE_ARTIFACT_MAX_EXPANDED_BYTES + 1).is_err()
        );
    }

    #[test]
    fn extraction_rejects_truncated_archives_and_nonempty_destinations() {
        let mut builder = tar::Builder::new(Vec::new());
        append_file(&mut builder, "file", 0o644, b"artifact contents");
        let mut bytes = builder.into_inner().expect("test archive should finish");
        bytes.truncate(520);
        let temporary = tempfile::tempdir().expect("temporary directory should exist");
        let artifact = temporary.path().join("source.tar");
        fs::write(&artifact, bytes).expect("test archive should write");
        let output = temporary.path().join("tree");
        fs::create_dir(&output).expect("output should exist");
        assert!(extract_verified_source_artifact(&artifact, &output).is_err());

        fs::write(&artifact, []).expect("test archive should write");
        fs::write(output.join("existing"), []).expect("destination fixture should write");
        assert!(matches!(
            extract_verified_source_artifact(&artifact, &output),
            Err(SourceArtifactError::Invalid(message)) if message.contains("not empty")
        ));
    }
}

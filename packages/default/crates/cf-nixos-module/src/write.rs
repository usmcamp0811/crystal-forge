//! Publishes a dedicated three-file artifact with one atomic directory rename.
//!
//! Preparation uses fresh files in a private sibling directory. Existing output
//! files are never opened for writing, including files with external hardlinks.
//! The final output component and existing entries must not be symlinks. Parent
//! path components are followed intentionally, as in the original writer.
//!
//! Callers must exclude concurrent namespace changes for validation and cleanup
//! guarantees. Descriptor-relative operations never write through an existing
//! file link, even if another process changes the namespace. Publication is not
//! a snapshot for readers that open multiple paths, nor an fsync durability
//! guarantee. Interrupted processes can leave a temporary sibling behind.

use std::ffi::OsStr;
use std::io::Write;
use std::os::fd::{AsFd, OwnedFd};
use std::path::{Component, Path};

use rustix::fs::{self, AtFlags, Dir, FileType, Mode, OFlags, RenameFlags, Stat};
use rustix::io::Errno;

use crate::generate::Generated;

const FILES: [&str; 3] = ["default.nix", "lib.nix", "manifest.json"];
const DIRECTORY_FLAGS: OFlags = OFlags::RDONLY
    .union(OFlags::DIRECTORY)
    .union(OFlags::NOFOLLOW)
    .union(OFlags::CLOEXEC);

#[cfg(test)]
std::thread_local! {
    // Fail at the publication boundary, after all staging and validation.
    static FAIL_PUBLICATION: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Publishes exactly `default.nix`, `lib.nix`, and `manifest.json` in `output`.
///
/// Existing output may contain only regular files with those names. Empty or
/// incomplete directories can be regenerated. Existing directory access bits
/// are retained; new output uses mode 0755. Ownership, ACLs, and special mode
/// bits are not copied. The parent must permit sibling creation and renaming.
///
/// # Errors
///
/// Returns an error for invalid artifacts, non-replaceable paths (including
/// `.`, `..`, and `/`), symlinks, unrelated entries, or filesystem failures.
/// Before publication, failures leave existing output unchanged. Linux atomic
/// exchange support is required for regeneration; there is no fallback.
/// Cleanup failure after publication is reported explicitly as already
/// published, with the retained temporary sibling named in the error.
pub fn write_output(generated: &Generated, output: &Path) -> Result<(), String> {
    // Inspect the literal final component: Path::components normalizes a
    // trailing '/.', which must not turn a request for '.' into its parent.
    let literal = output.as_os_str().as_encoded_bytes();
    let last = literal
        .rsplit(|byte| *byte == b'/')
        .find(|part| !part.is_empty());
    if matches!(last, None | Some(b".") | Some(b"..")) {
        return Err("output must name a replaceable child directory, not '.', '..', or '/'".into());
    }
    let Some(Component::Normal(name)) = output.components().next_back() else {
        return Err("output must name a replaceable child directory".into());
    };
    validate_generated(generated)?;
    let parent_path = output
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    std::fs::create_dir_all(parent_path)
        .map_err(|error| format!("could not create output parent: {error}"))?;
    // Parent symlinks are allowed; pin their resolved directory from now on.
    let parent = fs::open(
        parent_path,
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .map_err(|error| format!("could not open output parent: {error}"))?;
    let previous = inspect_output(&parent, name)?;

    // A private sibling keeps staging on the same filesystem. Flat cleanup
    // avoids traversing consumer-controlled directories.
    let staging_name = format!(".cf-nixos-module-{}", uuid::Uuid::new_v4());
    fs::mkdirat(&parent, staging_name.as_str(), Mode::from_raw_mode(0o700))
        .map_err(|error| format!("could not create staging directory: {error}"))?;
    let staged = fs::openat(
        &parent,
        staging_name.as_str(),
        DIRECTORY_FLAGS,
        Mode::empty(),
    );
    let result = (|| {
        let stage = staged
            .as_ref()
            .map_err(|error| format!("could not open staging directory: {error}"))?;
        for file in &generated.files {
            let fd = fs::openat(
                stage,
                file.path.as_str(),
                OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                Mode::from_raw_mode(0o644),
            )
            .map_err(|error| format!("could not create staged {}: {error}", file.path))?;
            let mut handle = std::fs::File::from(fd);
            handle
                .write_all(file.contents.as_bytes())
                .and_then(|()| handle.flush())
                .map_err(|error| format!("could not write staged {}: {error}", file.path))?;
        }
        validate_directory(stage, true)?;
        let mode = previous.as_ref().map_or(0o755, |stat| stat.st_mode & 0o777);
        fs::fchmod(stage, Mode::from_raw_mode(mode))
            .map_err(|error| format!("could not set output directory mode: {error}"))?;
        publish(&parent, &staging_name, stage, name, previous.as_ref())
    })();

    // After exchange the old output is at staging_name; before publication it
    // is the staged generation. Open only that final component without follow.
    // Never recursively remove unknown entries added by another process.
    let cleanup = (|| {
        let retired = if result.is_ok() {
            previous
        } else {
            match &staged {
                Ok(dir) => Some(fs::fstat(dir).map_err(|error| error.to_string())?),
                Err(_) => None,
            }
        };
        cleanup_sibling(&parent, &staging_name, retired.as_ref())
    })();
    match (result, cleanup) {
        (Ok(()), Ok(())) => Ok(()),
        (Ok(()), Err(error)) => Err(format!(
            "artifact published to {}; cleanup of {staging_name} failed: {error}",
            output.display()
        )),
        (Err(error), Ok(())) => Err(error),
        (Err(error), Err(cleanup)) => Err(format!(
            "{error}; cleanup of {staging_name} failed: {cleanup}"
        )),
    }
}

fn validate_generated(generated: &Generated) -> Result<(), String> {
    let mut names: Vec<_> = generated
        .files
        .iter()
        .map(|file| file.path.as_str())
        .collect();
    names.sort_unstable();
    if names != FILES {
        return Err("artifact must contain exactly default.nix, lib.nix, and manifest.json; refusing unsafe, duplicate, missing, or unknown generated paths".into());
    }
    Ok(())
}

fn inspect_output(parent: &impl AsFd, name: &OsStr) -> Result<Option<Stat>, String> {
    let stat = match fs::statat(parent, name, AtFlags::SYMLINK_NOFOLLOW) {
        Ok(stat) => stat,
        Err(Errno::NOENT) => return Ok(None),
        Err(error) => return Err(format!("could not inspect output safely: {error}")),
    };
    if FileType::from_raw_mode(stat.st_mode) != FileType::Directory {
        return Err("refusing output: not a directory (symlinks are not allowed)".into());
    }
    let dir = fs::openat(parent, name, DIRECTORY_FLAGS, Mode::empty())
        .map_err(|error| format!("could not open output directory safely: {error}"))?;
    let opened = fs::fstat(&dir).map_err(|error| error.to_string())?;
    if !same_inode(&stat, &opened) {
        return Err("output directory changed during inspection".into());
    }
    validate_directory(&dir, false)?;
    Ok(Some(opened))
}

// Existing output can be incomplete, but staging must have the full artifact.
fn validate_directory(dir: &impl AsFd, complete: bool) -> Result<(), String> {
    let entries =
        Dir::read_from(dir).map_err(|error| format!("could not list artifact: {error}"))?;
    let mut count = 0;
    for entry in entries {
        let entry = entry.map_err(|error| format!("could not read artifact entry: {error}"))?;
        let name = entry.file_name();
        if matches!(name.to_bytes(), b"." | b"..") {
            continue;
        }
        if !FILES.iter().any(|file| file.as_bytes() == name.to_bytes()) {
            return Err(format!(
                "refusing unrelated or stale artifact entry: {}",
                name.to_string_lossy()
            ));
        }
        let stat = fs::statat(dir, name, AtFlags::SYMLINK_NOFOLLOW)
            .map_err(|error| format!("could not inspect artifact entry: {error}"))?;
        if FileType::from_raw_mode(stat.st_mode) != FileType::RegularFile {
            return Err(format!(
                "refusing non-regular artifact entry (including symlink): {}",
                name.to_string_lossy()
            ));
        }
        count += 1;
    }
    if complete && count != FILES.len() {
        return Err("staged artifact is incomplete".into());
    }
    Ok(())
}

fn same_inode(left: &Stat, right: &Stat) -> bool {
    left.st_dev == right.st_dev && left.st_ino == right.st_ino
}

fn publish(
    parent: &impl AsFd,
    staging_name: &str,
    stage: &OwnedFd,
    output_name: &OsStr,
    previous: Option<&Stat>,
) -> Result<(), String> {
    let staged_stat = fs::fstat(stage).map_err(|error| error.to_string())?;
    let named_stat = fs::statat(parent, staging_name, AtFlags::SYMLINK_NOFOLLOW)
        .map_err(|error| format!("could not inspect staging before publication: {error}"))?;
    if !same_inode(&staged_stat, &named_stat) {
        return Err("staging directory changed before publication".into());
    }
    if let Some(previous) = previous {
        let current = inspect_output(parent, output_name)?;
        if !current
            .as_ref()
            .is_some_and(|current| same_inode(previous, current))
        {
            return Err("output directory changed before publication".into());
        }
    }
    // This is the sole commit point. NOREPLACE is an initial rename with an
    // absence guard: a newly appeared directory or symlink must not be lost.
    let flags = if previous.is_some() {
        RenameFlags::EXCHANGE
    } else {
        RenameFlags::NOREPLACE
    };
    #[cfg(test)]
    if FAIL_PUBLICATION.with(|fail| fail.replace(false)) {
        return Err("injected publication failure before renameat2".into());
    }
    fs::renameat_with(parent, staging_name, parent, output_name, flags)
        .map_err(|error| format!("could not publish artifact atomically (no fallback): {error}"))
}

fn cleanup_sibling(parent: &impl AsFd, name: &str, expected: Option<&Stat>) -> Result<(), String> {
    let Some(expected) = expected else {
        // Initial publication consumed staging, or opening an empty stage
        // failed. Only remove an empty directory; do not inspect its contents.
        return match fs::unlinkat(parent, name, AtFlags::REMOVEDIR) {
            Ok(()) | Err(Errno::NOENT) => Ok(()),
            Err(error) => Err(error.to_string()),
        };
    };
    let dir = match fs::openat(parent, name, DIRECTORY_FLAGS, Mode::empty()) {
        Ok(dir) => dir,
        Err(Errno::NOENT) => return Ok(()), // Initial publication consumed it.
        Err(error) => return Err(error.to_string()),
    };
    let stat = fs::fstat(&dir).map_err(|error| error.to_string())?;
    if !same_inode(expected, &stat) {
        return Err("temporary sibling changed before cleanup; refusing to remove it".into());
    }
    // The retired directory can have read-only access bits. Add owner access
    // only after publication (or to our unpublished stage) so cleanup works
    // without changing the permissions of the live output directory.
    let mode = stat.st_mode;
    if mode & 0o700 != 0o700 {
        fs::fchmod(&dir, Mode::from_raw_mode((mode & 0o777) | 0o700))
            .map_err(|error| error.to_string())?;
    }
    for file in FILES {
        match fs::unlinkat(&dir, file, AtFlags::empty()) {
            Ok(()) | Err(Errno::NOENT) => (),
            Err(error) => return Err(error.to_string()),
        }
    }
    fs::unlinkat(parent, name, AtFlags::REMOVEDIR).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::{MetadataExt, PermissionsExt, symlink};

    use super::*;
    use crate::generate::GeneratedFile;

    fn artifact(contents: &str) -> Generated {
        Generated {
            files: FILES
                .iter()
                .map(|name| GeneratedFile {
                    path: (*name).into(),
                    contents: format!("{name}: {contents}"),
                })
                .collect(),
            implemented: Vec::new(),
            skipped: Vec::new(),
            baseline: "test".into(),
        }
    }

    fn scratch() -> std::path::PathBuf {
        let base = std::env::temp_dir().join(format!("cf-writer-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&base).expect("scratch");
        base
    }

    fn assert_artifact(output: &Path, contents: &str) {
        for file in artifact(contents).files {
            assert_eq!(
                std::fs::read(output.join(file.path)).unwrap(),
                file.contents.as_bytes()
            );
        }
        assert_eq!(std::fs::read_dir(output).unwrap().count(), 3);
    }

    fn assert_no_staging(base: &Path) {
        for entry in std::fs::read_dir(base).unwrap() {
            assert!(
                !entry
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".cf-nixos-module-")
            );
        }
    }

    #[test]
    fn initial_publication_and_regeneration_replace_complete_directory() {
        let base = scratch();
        let output = base.join("out");
        write_output(&artifact("old"), &output).unwrap();
        assert_artifact(&output, "old");
        assert_eq!(std::fs::metadata(&output).unwrap().mode() & 0o777, 0o755);
        let old_inode = std::fs::metadata(&output).unwrap().ino();
        std::fs::set_permissions(&output, std::fs::Permissions::from_mode(0o750)).unwrap();
        write_output(&artifact("new"), &output).unwrap();
        assert_artifact(&output, "new");
        assert_ne!(std::fs::metadata(&output).unwrap().ino(), old_inode);
        assert_eq!(std::fs::metadata(&output).unwrap().mode() & 0o777, 0o750);
        assert_no_staging(&base);
        std::fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn preserves_read_only_directory_mode_and_cleans_retired_directory() {
        let base = scratch();
        let output = base.join("out");
        write_output(&artifact("old"), &output).unwrap();
        std::fs::set_permissions(&output, std::fs::Permissions::from_mode(0o555)).unwrap();
        write_output(&artifact("new"), &output).unwrap();
        assert_artifact(&output, "new");
        assert_eq!(std::fs::metadata(&output).unwrap().mode() & 0o777, 0o555);
        assert_no_staging(&base);
        std::fs::set_permissions(&output, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn regenerates_empty_and_incomplete_directories() {
        let base = scratch();
        let output = base.join("out");
        std::fs::create_dir(&output).unwrap();
        write_output(&artifact("empty"), &output).unwrap();
        std::fs::remove_file(output.join("manifest.json")).unwrap();
        write_output(&artifact("complete"), &output).unwrap();
        assert_artifact(&output, "complete");
        assert_no_staging(&base);
        std::fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn external_hardlinks_are_not_overwritten() {
        let base = scratch();
        let output = base.join("out");
        write_output(&artifact("old"), &output).unwrap();
        for file in FILES {
            std::fs::hard_link(output.join(file), base.join(file)).unwrap();
        }
        write_output(&artifact("new"), &output).unwrap();
        assert_artifact(&output, "new");
        for file in artifact("old").files {
            assert_eq!(
                std::fs::read(base.join(file.path)).unwrap(),
                file.contents.as_bytes()
            );
        }
        assert_no_staging(&base);
        std::fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn refuses_final_output_symlink_even_with_trailing_slash() {
        let base = scratch();
        let outside = base.join("outside");
        write_output(&artifact("old"), &outside).unwrap();
        let output = base.join("out");
        symlink(&outside, &output).unwrap();
        for path in [output.clone(), output.join("")] {
            assert!(
                write_output(&artifact("new"), &path)
                    .unwrap_err()
                    .contains("symlink")
            );
        }
        assert_artifact(&outside, "old");
        assert!(output.is_symlink());
        assert_no_staging(&base);
        std::fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn rejects_symlinks_at_each_artifact_name_without_changing_other_files() {
        let base = scratch();
        let output = base.join("out");
        let outside = base.join("outside");
        std::fs::write(&outside, "canary").unwrap();
        for name in FILES {
            write_output(&artifact("old"), &output).unwrap();
            std::fs::remove_file(output.join(name)).unwrap();
            symlink(&outside, output.join(name)).unwrap();
            assert!(
                write_output(&artifact("new"), &output)
                    .unwrap_err()
                    .contains("symlink")
            );
            assert_eq!(std::fs::read_to_string(&outside).unwrap(), "canary");
            for other in FILES.into_iter().filter(|other| *other != name) {
                assert_eq!(
                    std::fs::read_to_string(output.join(other)).unwrap(),
                    format!("{other}: old")
                );
            }
            std::fs::remove_file(output.join(name)).unwrap();
        }
        assert_no_staging(&base);
        std::fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn refuses_unrelated_files_directories_and_stale_policies() {
        let base = scratch();
        let output = base.join("out");
        write_output(&artifact("old"), &output).unwrap();
        for name in ["notes.txt", "policies"] {
            let extra = output.join(name);
            std::fs::create_dir(&extra).unwrap();
            std::fs::write(extra.join("canary"), "keep").unwrap();
            assert!(
                write_output(&artifact("new"), &output)
                    .unwrap_err()
                    .contains("stale")
            );
            assert_eq!(
                std::fs::read_to_string(extra.join("canary")).unwrap(),
                "keep"
            );
            std::fs::remove_dir_all(&extra).unwrap();
            std::fs::write(&extra, "keep").unwrap();
            assert!(write_output(&artifact("new"), &output).is_err());
            assert_eq!(std::fs::read_to_string(&extra).unwrap(), "keep");
            std::fs::remove_file(extra).unwrap();
            assert_artifact(&output, "old");
        }
        assert_no_staging(&base);
        std::fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn refuses_stale_directory_symlink_without_traversing_it() {
        let base = scratch();
        let output = base.join("out");
        let outside = base.join("outside");
        std::fs::create_dir(&outside).unwrap();
        std::fs::write(outside.join("canary"), "keep").unwrap();
        write_output(&artifact("old"), &output).unwrap();
        symlink(&outside, output.join("policies")).unwrap();
        assert!(write_output(&artifact("new"), &output).is_err());
        assert!(output.join("policies").is_symlink());
        assert_eq!(
            std::fs::read_to_string(outside.join("canary")).unwrap(),
            "keep"
        );
        assert_eq!(std::fs::read_dir(&outside).unwrap().count(), 1);
        std::fs::remove_file(output.join("policies")).unwrap();
        assert_artifact(&output, "old");
        // A generated target must also not be an ordinary directory.
        std::fs::remove_file(output.join("manifest.json")).unwrap();
        std::fs::create_dir(output.join("manifest.json")).unwrap();
        assert!(write_output(&artifact("new"), &output).is_err());
        assert!(output.join("manifest.json").is_dir());
        assert_no_staging(&base);
        std::fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn rejects_invalid_generated_artifacts_before_touching_output() {
        let base = scratch();
        let output = base.join("out");
        write_output(&artifact("old"), &output).unwrap();
        for bad in [
            "../escape",
            "policies/x.nix",
            "",
            ".",
            "..",
            "lib.nix",
            "other",
            "a\\b",
            "a\0b",
        ] {
            let mut generated = artifact("new");
            generated.files[2].path = bad.into();
            assert!(write_output(&generated, &output).is_err());
            assert_artifact(&output, "old");
        }
        let mut missing = artifact("new");
        missing.files.pop();
        assert!(write_output(&missing, &output).is_err());
        assert_artifact(&output, "old");
        assert_no_staging(&base);
        std::fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn refuses_nonreplaceable_output_paths() {
        for path in ["", ".", "..", "/", "somewhere/.", "somewhere/../", "./"] {
            assert!(
                write_output(&artifact("new"), Path::new(path))
                    .unwrap_err()
                    .contains("replaceable")
            );
        }
    }

    #[test]
    fn failed_initial_publication_does_not_replace_a_newly_appeared_directory() {
        let base = scratch();
        let parent = fs::open(&base, DIRECTORY_FLAGS, Mode::empty()).unwrap();
        write_output(&artifact("new"), &base.join("stage")).unwrap();
        let stage = fs::openat(&parent, "stage", DIRECTORY_FLAGS, Mode::empty()).unwrap();
        std::fs::create_dir(base.join("out")).unwrap();
        let error = publish(&parent, "stage", &stage, OsStr::new("out"), None).unwrap_err();
        assert!(error.contains("no fallback"));
        assert_eq!(std::fs::read_dir(base.join("out")).unwrap().count(), 0);
        assert_artifact(&base.join("stage"), "new");
        let stat = fs::fstat(&stage).unwrap();
        cleanup_sibling(&parent, "stage", Some(&stat)).unwrap();
        assert!(!base.join("stage").exists());
        std::fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn failed_exchange_leaves_original_byte_identical() {
        let base = scratch();
        let output = base.join("out");
        write_output(&artifact("old"), &output).unwrap();
        let original = std::fs::metadata(&output).unwrap();
        FAIL_PUBLICATION.with(|fail| fail.set(true));
        let error = write_output(&artifact("new"), &output).unwrap_err();
        assert!(error.contains("injected publication failure"));
        assert_artifact(&output, "old");
        assert_eq!(std::fs::metadata(&output).unwrap().ino(), original.ino());
        assert_no_staging(&base);
        std::fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn failed_first_publication_leaves_output_absent_and_cleans_staging() {
        let base = scratch();
        let output = base.join("out");
        FAIL_PUBLICATION.with(|fail| fail.set(true));
        assert!(
            write_output(&artifact("new"), &output)
                .unwrap_err()
                .contains("injected publication failure")
        );
        assert!(!output.exists());
        assert_no_staging(&base);
        std::fs::remove_dir_all(base).unwrap();
    }
}

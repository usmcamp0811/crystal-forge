//! Symlink-safe artifact writing.
//!
//! The generator must never modify anything outside `--output`, including
//! through symlinks that already exist inside the output directory. A path-only
//! check (rejecting `..` and absolute paths) is insufficient: it neither
//! detects a pre-existing symlink nor closes the window between checking and
//! writing.
//!
//! This module instead opens the output directory once, without following a
//! symlink in its final component, and writes every file *relative to that
//! descriptor* with `O_NOFOLLOW`. A symlink at any target name therefore fails
//! the open outright rather than being written through, and there is no
//! check-then-write race.

use std::io::Write;
use std::path::Path;

use rustix::fs::{Mode, OFlags, openat};

use crate::generate::Generated;

/// Write every generated file into `output`.
pub fn write_output(generated: &Generated, output: &Path) -> Result<(), String> {
    // Create the output directory (and any missing parents). Parent components
    // of a user-supplied output path are followed intentionally; only the
    // output directory itself and its contents are protected.
    std::fs::create_dir_all(output)
        .map_err(|error| format!("could not create {}: {error}", output.display()))?;

    // `policies/` was the generator-owned directory in the previous artifact
    // layout. Reject it rather than leaving stale policy modules beside the new
    // three-file artifact. Rejection is safer than recursively deleting a path
    // that may have been supplied by the consumer, and symlink_metadata keeps a
    // symlink from being followed during this check.
    let stale_policies = output.join("policies");
    if std::fs::symlink_metadata(&stale_policies).is_ok() {
        return Err(format!(
            "refusing to regenerate {}: stale generator-owned policies/ output exists; remove it or choose a clean output directory",
            output.display()
        ));
    }

    // Open the output directory without following a final symlink. If `output`
    // itself is a symlink, this fails instead of writing through it.
    let dir = rustix::fs::open(
        output,
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .map_err(|error| {
        format!(
            "could not open output directory {} safely: {error}. \
             If it is a symlink, pass the real directory instead.",
            output.display()
        )
    })?;

    for file in &generated.files {
        write_file_at(&dir, &file.path, file.contents.as_bytes(), output)?;
    }

    Ok(())
}

/// Write one file relative to the output directory descriptor.
fn write_file_at(
    dir: &impl std::os::fd::AsFd,
    relative_path: &str,
    contents: &[u8],
    output_display: &Path,
) -> Result<(), String> {
    // The generated layout is flat by construction. Rejecting any separator
    // keeps that invariant explicit and means we never traverse a directory we
    // did not create in this call.
    if relative_path.is_empty()
        || relative_path.contains('/')
        || relative_path.contains('\\')
        || relative_path.contains('\0')
        || relative_path == "."
        || relative_path == ".."
    {
        return Err(format!(
            "refusing to write unsafe generated path: {relative_path}"
        ));
    }

    let fd = openat(
        dir,
        relative_path,
        OFlags::WRONLY | OFlags::CREATE | OFlags::TRUNC | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::from_raw_mode(0o644),
    )
    .map_err(|error| {
        if error == rustix::io::Errno::LOOP {
            format!(
                "refusing to write {} in {}: it is a symlink. \
                 The generator never writes through symlinks.",
                relative_path,
                output_display.display()
            )
        } else {
            format!(
                "could not write {} in {}: {error}",
                relative_path,
                output_display.display()
            )
        }
    })?;

    let mut handle = std::fs::File::from(fd);
    handle
        .write_all(contents)
        .map_err(|error| format!("could not write {relative_path}: {error}"))?;
    handle
        .flush()
        .map_err(|error| format!("could not flush {relative_path}: {error}"))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::symlink;

    use super::*;
    use crate::generate::{Generated, GeneratedFile};

    fn artifact(files: Vec<(&str, &str)>) -> Generated {
        Generated {
            files: files
                .into_iter()
                .map(|(path, contents)| GeneratedFile {
                    path: path.to_string(),
                    contents: contents.to_string(),
                })
                .collect(),
            implemented: Vec::new(),
            skipped: Vec::new(),
            baseline: "test".to_string(),
        }
    }

    /// Create a unique scratch directory for one test.
    fn scratch(name: &str) -> std::path::PathBuf {
        let base = std::env::temp_dir().join(format!(
            "cf-nixos-module-{}-{}-{:?}",
            name,
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).expect("create scratch dir");
        base
    }

    #[test]
    fn writes_files_into_the_output_directory() {
        let base = scratch("write-ok");
        let output = base.join("out");

        write_output(&artifact(vec![("default.nix", "{ }")]), &output).expect("writes");

        assert_eq!(
            std::fs::read_to_string(output.join("default.nix")).expect("read"),
            "{ }"
        );
        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn overwrites_a_previous_regular_file() {
        let base = scratch("write-overwrite");
        let output = base.join("out");
        std::fs::create_dir_all(&output).expect("mkdir");
        std::fs::write(output.join("default.nix"), "stale contents").expect("seed");

        write_output(&artifact(vec![("default.nix", "fresh")]), &output).expect("writes");

        assert_eq!(
            std::fs::read_to_string(output.join("default.nix")).expect("read"),
            "fresh"
        );
        std::fs::remove_dir_all(&base).ok();
    }

    /// `output/default.nix -> file outside output` must never be written through.
    #[test]
    fn refuses_to_write_through_a_file_symlink() {
        let base = scratch("symlink-file");
        let output = base.join("out");
        let outside = base.join("outside.nix");
        std::fs::create_dir_all(&output).expect("mkdir");
        std::fs::write(&outside, "ORIGINAL").expect("seed outside file");
        symlink(&outside, output.join("default.nix")).expect("create symlink");

        let error = write_output(&artifact(vec![("default.nix", "MALICIOUS")]), &output)
            .expect_err("must refuse to follow the symlink");
        assert!(error.contains("symlink"), "unexpected error: {error}");

        // The outside target must be untouched.
        assert_eq!(
            std::fs::read_to_string(&outside).expect("read outside"),
            "ORIGINAL"
        );
        std::fs::remove_dir_all(&base).ok();
    }

    /// A symlink in the generator-owned previous layout is rejected before any
    /// output file is touched.
    #[test]
    fn never_writes_into_a_directory_symlink_inside_the_output() {
        let base = scratch("symlink-dir");
        let output = base.join("out");
        let outside_dir = base.join("outside-dir");
        std::fs::create_dir_all(&output).expect("mkdir out");
        std::fs::create_dir_all(&outside_dir).expect("mkdir outside");
        std::fs::write(outside_dir.join("canary"), "ORIGINAL").expect("seed canary");
        symlink(&outside_dir, output.join("policies")).expect("create dir symlink");

        let error = write_output(
            &artifact(vec![("default.nix", "{ }"), ("manifest.json", "{}")]),
            &output,
        )
        .expect_err("stale generator-owned symlink must be rejected");
        assert!(error.contains("stale"), "unexpected error: {error}");

        // Nothing was added to or changed in the outside directory.
        assert_eq!(
            std::fs::read_to_string(outside_dir.join("canary")).expect("read canary"),
            "ORIGINAL"
        );
        let outside_entries: Vec<_> = std::fs::read_dir(&outside_dir)
            .expect("read outside dir")
            .map(|entry| entry.expect("entry").file_name())
            .collect();
        assert_eq!(outside_entries.len(), 1, "outside dir gained files");
        std::fs::remove_dir_all(&base).ok();
    }

    /// The output directory itself being a symlink must be refused.
    #[test]
    fn refuses_an_output_directory_that_is_a_symlink() {
        let base = scratch("symlink-output");
        let real_output = base.join("real");
        let outside_dir = base.join("outside-dir");
        std::fs::create_dir_all(&outside_dir).expect("mkdir outside");
        symlink(&outside_dir, &real_output).expect("create symlink");

        let error = write_output(&artifact(vec![("default.nix", "x")]), &real_output)
            .expect_err("must refuse a symlinked output directory");
        assert!(error.contains("safely"), "unexpected error: {error}");

        assert!(
            std::fs::read_dir(&outside_dir)
                .expect("read outside")
                .next()
                .is_none(),
            "outside directory was written to"
        );
        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn rejects_generated_paths_containing_separators() {
        let base = scratch("bad-path");
        let output = base.join("out");

        for bad in ["policies/x.nix", "../escape.nix", "..", ".", ""] {
            let error = write_output(&artifact(vec![(bad, "x")]), &output)
                .expect_err("must reject unsafe generated path");
            assert!(
                error.contains("unsafe"),
                "unexpected error for {bad}: {error}"
            );
        }
        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn rejects_stale_previous_layout() {
        let base = scratch("stale-layout");
        let output = base.join("out");
        std::fs::create_dir_all(output.join("policies")).expect("mkdir stale layout");
        std::fs::write(output.join("policies/stale.nix"), "stale").expect("seed stale file");

        let error = write_output(&artifact(vec![("default.nix", "fresh")]), &output)
            .expect_err("stale previous layout must be rejected");
        assert!(error.contains("stale"), "unexpected error: {error}");
        assert!(output.join("policies/stale.nix").exists());
        std::fs::remove_dir_all(&base).ok();
    }
}

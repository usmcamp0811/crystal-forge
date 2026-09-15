//! Publishes authorized Git commits as canonical source artifacts and store paths.
//!
//! The server applies Git credentials only while updating its bare mirror. It
//! then creates one tracked-tree tar artifact from the exact commit. The server
//! and API builder both validate and extract those same bytes through
//! [`cf_protocol::source_artifact`] before Nix store ingestion.

use std::fs::{File, OpenOptions};
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use anyhow::{Context, Result, anyhow};
use cf_protocol::builder::{
    ImmutableSourceIdentity, VERIFIED_SOURCE_MATERIALIZATION_SCHEMA_VERSION,
};
use cf_protocol::source_artifact::{
    SourceArtifactError, VERIFIED_SOURCE_ARTIFACT_FORMAT_VERSION,
    VERIFIED_SOURCE_ARTIFACT_MAX_BYTES, extract_verified_source_artifact,
};
use dashmap::DashMap;
#[allow(deprecated)]
use nix::fcntl::{FlockArg, flock};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tokio::sync::Mutex;

use crate::flake::credentials::FlakeCredentialEnv;

static MIRROR_LOCKS: OnceLock<DashMap<String, Arc<Mutex<()>>>> = OnceLock::new();

/// Classifies canonical source publication failures for retry policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MaterializationFailureClass {
    /// No contract-v1 publication exists for a pre-upgrade queued job.
    NotPublished,
    /// The Git object format is not supported by materialization schema v1.
    UnsupportedObjectFormat,
    /// The source identity or tracked tree cannot satisfy the contract.
    Deterministic,
    /// Local I/O, Git transport, or Nix infrastructure can recover on retry.
    Transient,
    /// The user requested cancellation before publication completed.
    Cancelled,
}

/// Reports a classified canonical source publication failure.
#[derive(Debug)]
pub struct MaterializationError {
    /// Retry classification for the failed operation.
    pub class: MaterializationFailureClass,
    source: anyhow::Error,
}

impl MaterializationError {
    fn not_published(error: impl Into<anyhow::Error>) -> Self {
        Self {
            class: MaterializationFailureClass::NotPublished,
            source: error.into(),
        }
    }

    fn unsupported_object_format(error: impl Into<anyhow::Error>) -> Self {
        Self {
            class: MaterializationFailureClass::UnsupportedObjectFormat,
            source: error.into(),
        }
    }

    fn deterministic(error: impl Into<anyhow::Error>) -> Self {
        Self {
            class: MaterializationFailureClass::Deterministic,
            source: error.into(),
        }
    }

    fn transient(error: impl Into<anyhow::Error>) -> Self {
        Self {
            class: MaterializationFailureClass::Transient,
            source: error.into(),
        }
    }

    fn cancelled() -> Self {
        Self {
            class: MaterializationFailureClass::Cancelled,
            source: anyhow!("canonical source materialization was cancelled"),
        }
    }
}

impl std::fmt::Display for MaterializationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}", self.source)
    }
}

impl std::error::Error for MaterializationError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.source.source()
    }
}

/// Couples a published identity with its server-local artifact path.
#[derive(Debug, Clone)]
pub struct PublishedSource {
    /// Server-authorized identity derived from the canonical artifact.
    pub identity: ImmutableSourceIdentity,
    /// Server-local path to the exact artifact bytes.
    pub artifact_path: PathBuf,
}

/// Returns the stable identifier for a repository mirror.
pub fn source_mirror_id(repo_url: &str) -> String {
    let digest = Sha256::digest(repo_url.as_bytes());
    format!("repo-{}", hex::encode(&digest[..12]))
}

/// Returns the shared server bare-mirror path for a repository.
pub fn server_mirror_path(source_root: &Path, repo_url: &str) -> PathBuf {
    source_root
        .join("mirrors")
        .join(format!("{}.git", source_mirror_id(repo_url)))
}

/// Returns the process-wide lock that serializes one repository mirror.
///
/// Callers MUST acquire this lock before the cross-process file lock. They MUST
/// hold both locks across mirror mutation and canonical artifact publication.
pub fn mirror_lock(mirror_id: &str) -> Arc<Mutex<()>> {
    MIRROR_LOCKS
        .get_or_init(DashMap::new)
        .entry(mirror_id.to_string())
        .or_insert_with(|| Arc::new(Mutex::new(())))
        .clone()
}

fn source_store_name(commit_hash: &str) -> Result<String, MaterializationError> {
    if commit_hash.len() == 64 && commit_hash.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(MaterializationError::unsupported_object_format(anyhow!(
            "Git SHA-256 object IDs are unsupported by verified-source materialization schema version 1"
        )));
    }
    if commit_hash.len() != 40 || !commit_hash.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(MaterializationError::deterministic(anyhow!(
            "authorized commit identity is not a full 40-character SHA-1 Git object ID"
        )));
    }
    Ok(format!(
        "crystal-forge-source-v{}-{commit_hash}",
        VERIFIED_SOURCE_MATERIALIZATION_SCHEMA_VERSION
    ))
}

fn artifact_path(source_root: &Path, mirror_id: &str, commit_hash: &str) -> PathBuf {
    source_root
        .join("artifacts")
        .join(mirror_id)
        .join(format!("{commit_hash}.tar"))
}

fn identity_path(source_root: &Path, mirror_id: &str, commit_hash: &str) -> PathBuf {
    source_root
        .join("identities")
        .join(mirror_id)
        .join(format!("{commit_hash}.json"))
}

fn lock_path(source_root: &Path, mirror_id: &str) -> PathBuf {
    source_root.join("locks").join(format!("{mirror_id}.lock"))
}

async fn command_output(command: &mut Command, operation: &str) -> Result<std::process::Output> {
    let output = tokio::time::timeout(std::time::Duration::from_secs(120), command.output())
        .await
        .with_context(|| format!("{operation} timed out"))?
        .with_context(|| format!("failed to start {operation}"))?;
    if !output.status.success() {
        let stderr = crate::security::snapshot_redaction::redact_text(
            String::from_utf8_lossy(&output.stderr).trim(),
        );
        anyhow::bail!("{operation} failed: {stderr}");
    }
    Ok(output)
}

fn hardened_git_command() -> Command {
    let mut command = Command::new("git");
    // SECURITY: These commands use only explicit arguments and tracked tree
    // attributes. Host and mirror config must not redirect URLs or run helpers.
    command
        .kill_on_drop(true)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG", "/dev/null")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .args([
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "credential.helper=",
            "-c",
            "protocol.ext.allow=never",
            "-c",
            "protocol.file.allow=user",
        ]);
    command
}

async fn ensure_materialization_not_cancelled(
    pool: &PgPool,
    commit_id: i32,
) -> Result<(), MaterializationError> {
    match crate::queries::commits::check_cancellation_requested(pool, commit_id).await {
        Ok(true) => Err(MaterializationError::cancelled()),
        Ok(false) => Ok(()),
        Err(error) => Err(MaterializationError::transient(
            anyhow!(error).context("failed to check source materialization cancellation"),
        )),
    }
}

async fn acquire_repository_file_lock(path: &Path) -> Result<File> {
    let parent = path.parent().context("source lock path has no parent")?;
    tokio::fs::create_dir_all(parent).await?;
    let path = path.to_path_buf();
    tokio::task::spawn_blocking(move || {
        let file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .open(&path)?;
        #[allow(deprecated)]
        flock(file.as_raw_fd(), FlockArg::LockExclusive)?;
        Ok::<_, anyhow::Error>(file)
    })
    .await
    .context("source artifact file-lock task failed")?
}

async fn ensure_mirror_has_commit(
    mirror_path: &Path,
    repo_url: &str,
    commit_hash: &str,
    credentials: Option<&FlakeCredentialEnv>,
) -> Result<()> {
    if !mirror_path.exists() {
        let parent = mirror_path
            .parent()
            .context("source mirror path has no parent")?;
        tokio::fs::create_dir_all(parent).await?;
        let temporary = parent.join(format!(".mirror-{}", uuid::Uuid::new_v4()));
        let mut command = hardened_git_command();
        command.args(["init", "--bare"]).arg(&temporary);
        if let Err(error) = command_output(&mut command, "Git mirror initialization").await {
            let _ = tokio::fs::remove_dir_all(&temporary).await;
            return Err(error);
        }
        if let Err(error) = tokio::fs::rename(&temporary, mirror_path).await {
            let _ = tokio::fs::remove_dir_all(&temporary).await;
            return Err(error.into());
        }
    }

    let mut verify = hardened_git_command();
    verify.arg("--git-dir").arg(mirror_path).args([
        "cat-file",
        "-e",
        &format!("{commit_hash}^{{commit}}"),
    ]);
    if verify
        .output()
        .await
        .is_ok_and(|output| output.status.success())
    {
        return Ok(());
    }

    let mut fetch = hardened_git_command();
    fetch
        .arg("--git-dir")
        .arg(mirror_path)
        .args(["fetch", "--prune"])
        .arg(repo_url)
        .arg("+refs/*:refs/*");
    if let Some(credentials) = credentials {
        credentials.apply_to_git_command(&mut fetch);
    }
    command_output(&mut fetch, "authorized Git commit fetch").await?;
    command_output(&mut verify, "authorized Git commit verification").await?;
    Ok(())
}

async fn sha256_file(path: &Path) -> Result<String> {
    let mut file = tokio::fs::File::open(path).await?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).await?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hex::encode(hasher.finalize()))
}

async fn path_nar_hash(path: &Path) -> Result<String> {
    let mut command = Command::new("nix");
    command
        .kill_on_drop(true)
        .args(["hash", "path", "--type", "sha256", "--sri"])
        .arg(path);
    let output = command_output(&mut command, "Nix source NAR hash").await?;
    Ok(String::from_utf8(output.stdout)?.trim().to_string())
}

async fn lock_hash(path: &Path) -> Result<String, MaterializationError> {
    let bytes = tokio::fs::read(path.join("flake.lock"))
        .await
        .map_err(|error| {
            MaterializationError::deterministic(
                anyhow!(error).context("authorized source has no readable flake.lock"),
            )
        })?;
    Ok(hex::encode(Sha256::digest(bytes)))
}

fn validate_published_identity(
    identity: ImmutableSourceIdentity,
    expected_store_name: &str,
    artifact_path: &Path,
) -> Result<PublishedSource, MaterializationError> {
    let metadata = std::fs::metadata(artifact_path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            MaterializationError::not_published(
                anyhow!(error).context("canonical source artifact is not published"),
            )
        } else {
            MaterializationError::transient(
                anyhow!(error).context("canonical source artifact is unreadable"),
            )
        }
    })?;
    if identity.schema_version != VERIFIED_SOURCE_MATERIALIZATION_SCHEMA_VERSION
        || identity.store_name != expected_store_name
        || identity.artifact_format_version != VERIFIED_SOURCE_ARTIFACT_FORMAT_VERSION
        || identity.artifact_size != metadata.len()
        || identity.artifact_size > VERIFIED_SOURCE_ARTIFACT_MAX_BYTES
    {
        return Err(MaterializationError::deterministic(anyhow!(
            "published source identity does not satisfy contract version 1"
        )));
    }
    Ok(PublishedSource {
        identity,
        artifact_path: artifact_path.to_path_buf(),
    })
}

/// Loads a previously published canonical artifact without Git or Nix work.
///
/// This lookup performs only bounded JSON, metadata, and artifact digest reads.
/// It is suitable for dispatch preflight before a build job is committed as
/// claimed. It does not execute Git or Nix.
///
/// # Errors
///
/// Returns a not-published error when the identity is absent and a transient
/// error when it is unreadable. Returns a deterministic error when published
/// metadata violates contract version 1.
pub async fn lookup_published_source(
    source_root: &Path,
    repo_url: &str,
    commit_hash: &str,
) -> Result<PublishedSource, MaterializationError> {
    let store_name = source_store_name(commit_hash)?;
    let mirror_id = source_mirror_id(repo_url);
    let identity_bytes = tokio::fs::read(identity_path(source_root, &mirror_id, commit_hash))
        .await
        .map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                MaterializationError::not_published(
                    anyhow!(error).context("canonical source identity is not published"),
                )
            } else {
                MaterializationError::transient(
                    anyhow!(error).context("canonical source identity is unreadable"),
                )
            }
        })?;
    let identity =
        serde_json::from_slice::<ImmutableSourceIdentity>(&identity_bytes).map_err(|error| {
            MaterializationError::deterministic(
                anyhow!(error).context("canonical source identity is malformed"),
            )
        })?;
    let published = validate_published_identity(
        identity,
        &store_name,
        &artifact_path(source_root, &mirror_id, commit_hash),
    )?;
    let digest = sha256_file(&published.artifact_path)
        .await
        .map_err(MaterializationError::transient)?;
    if digest != published.identity.artifact_sha256 {
        return Err(MaterializationError::deterministic(anyhow!(
            "canonical source artifact digest differs from its published identity"
        )));
    }
    Ok(published)
}

/// Publishes one authorized commit as a canonical artifact and Nix store source.
///
/// CONCURRENCY: The function acquires the process mutex and then the repository
/// file lock. Every server process uses this ordering. Identity JSON is renamed
/// last, so dispatch never observes partial artifact publication.
///
/// # Errors
///
/// Returns an unsupported-object-format error for a SHA-256 Git object ID.
/// Returns a deterministic error for another invalid commit ID, unsafe artifact,
/// or missing `flake.lock`. Returns a transient error for Git, filesystem, or
/// Nix infrastructure failures. Returns a cancelled error when the commit enters
/// cancellation before publication completes.
pub async fn materialize_immutable_source(
    pool: &PgPool,
    commit_id: i32,
    source_root: &Path,
    repo_url: &str,
    commit_hash: &str,
    credentials: Option<&FlakeCredentialEnv>,
) -> Result<ImmutableSourceIdentity, MaterializationError> {
    ensure_materialization_not_cancelled(pool, commit_id).await?;
    let store_name = source_store_name(commit_hash)?;
    let mirror_id = source_mirror_id(repo_url);
    let _process_guard = mirror_lock(&mirror_id).lock_owned().await;
    let _file_guard = acquire_repository_file_lock(&lock_path(source_root, &mirror_id))
        .await
        .map_err(MaterializationError::transient)?;
    ensure_materialization_not_cancelled(pool, commit_id).await?;

    if let Ok(published) = lookup_published_source(source_root, repo_url, commit_hash).await {
        let store_path = Path::new(&published.identity.server_store_path);
        if store_path.exists()
            && path_nar_hash(store_path).await.ok().as_deref()
                == Some(published.identity.nar_hash.as_str())
            && lock_hash(store_path).await.ok().as_deref()
                == Some(published.identity.lock_hash.as_str())
        {
            return Ok(published.identity);
        }
    }

    let mirror_path = server_mirror_path(source_root, repo_url);
    ensure_mirror_has_commit(&mirror_path, repo_url, commit_hash, credentials)
        .await
        .map_err(MaterializationError::transient)?;
    ensure_materialization_not_cancelled(pool, commit_id).await?;

    let staging_parent = source_root.join("staging");
    tokio::fs::create_dir_all(&staging_parent)
        .await
        .map_err(MaterializationError::transient)?;
    let staging = tempfile::Builder::new()
        .prefix("source-")
        .tempdir_in(&staging_parent)
        .map_err(MaterializationError::transient)?;
    let temporary_artifact = staging.path().join("source.tar");
    let tree_path = staging.path().join("tree");
    tokio::fs::create_dir(&tree_path)
        .await
        .map_err(MaterializationError::transient)?;

    let mut archive = hardened_git_command();
    archive
        .arg("--git-dir")
        .arg(&mirror_path)
        .args(["archive", "--format=tar", "--output"])
        .arg(&temporary_artifact)
        .arg(commit_hash);
    command_output(&mut archive, "authorized Git tree artifact generation")
        .await
        .map_err(MaterializationError::transient)?;
    ensure_materialization_not_cancelled(pool, commit_id).await?;
    let artifact_size = tokio::fs::metadata(&temporary_artifact)
        .await
        .map_err(MaterializationError::transient)?
        .len();
    if artifact_size > VERIFIED_SOURCE_ARTIFACT_MAX_BYTES {
        return Err(MaterializationError::deterministic(anyhow!(
            "canonical source artifact exceeds {} bytes",
            VERIFIED_SOURCE_ARTIFACT_MAX_BYTES
        )));
    }
    let artifact_sha256 = sha256_file(&temporary_artifact)
        .await
        .map_err(MaterializationError::transient)?;
    let artifact_for_extract = temporary_artifact.clone();
    let tree_for_extract = tree_path.clone();
    tokio::task::spawn_blocking(move || {
        extract_verified_source_artifact(&artifact_for_extract, &tree_for_extract)
    })
    .await
    .map_err(|error| MaterializationError::transient(anyhow!(error)))?
    .map_err(|error| match error {
        SourceArtifactError::Invalid(_) => MaterializationError::deterministic(anyhow!(error)),
        SourceArtifactError::Io(_) => MaterializationError::transient(anyhow!(error)),
    })?;
    ensure_materialization_not_cancelled(pool, commit_id).await?;
    let lock_hash = lock_hash(&tree_path).await?;

    let mut add = Command::new("nix");
    add.kill_on_drop(true)
        .args(["store", "add-path", "--name", &store_name])
        .arg(&tree_path);
    let output = command_output(&mut add, "Nix immutable source ingestion")
        .await
        .map_err(MaterializationError::transient)?;
    ensure_materialization_not_cancelled(pool, commit_id).await?;
    let store_path = String::from_utf8(output.stdout)
        .map_err(|error| MaterializationError::transient(anyhow!(error)))?
        .trim()
        .to_string();
    if !store_path.starts_with("/nix/store/") || !store_path.ends_with(&format!("-{store_name}")) {
        return Err(MaterializationError::deterministic(anyhow!(
            "Nix source ingestion returned an unexpected store path"
        )));
    }
    let nar_hash = path_nar_hash(Path::new(&store_path))
        .await
        .map_err(MaterializationError::transient)?;
    let identity = ImmutableSourceIdentity {
        schema_version: VERIFIED_SOURCE_MATERIALIZATION_SCHEMA_VERSION,
        store_name,
        nar_hash,
        lock_hash,
        artifact_format_version: VERIFIED_SOURCE_ARTIFACT_FORMAT_VERSION,
        artifact_sha256,
        artifact_size,
        server_store_path: store_path,
    };

    let final_artifact = artifact_path(source_root, &mirror_id, commit_hash);
    let final_identity = identity_path(source_root, &mirror_id, commit_hash);
    if let Some(parent) = final_artifact.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(MaterializationError::transient)?;
    }
    if let Some(parent) = final_identity.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(MaterializationError::transient)?;
    }
    tokio::fs::rename(&temporary_artifact, &final_artifact)
        .await
        .map_err(MaterializationError::transient)?;
    let temporary_identity =
        final_identity.with_extension(format!("json.tmp-{}", uuid::Uuid::new_v4()));
    tokio::fs::write(
        &temporary_identity,
        serde_json::to_vec(&identity).map_err(MaterializationError::transient)?,
    )
    .await
    .map_err(MaterializationError::transient)?;
    // The identity rename activates publication for dispatch readers.
    ensure_materialization_not_cancelled(pool, commit_id).await?;
    tokio::fs::rename(&temporary_identity, &final_identity)
        .await
        .map_err(MaterializationError::transient)?;
    Ok(identity)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_test_git(directory: &Path, arguments: &[&str]) -> std::process::Output {
        std::process::Command::new("git")
            .current_dir(directory)
            .args(arguments)
            .output()
            .expect("test Git command should start")
    }

    #[test]
    fn store_name_requires_full_commit_input() {
        assert!(source_store_name("not/a/commit").is_err());
        assert!(source_store_name("86c0285").is_err());
        assert_eq!(
            source_store_name("86c02858f73f0650c86a567a8c240378e31f2979")
                .expect("full object IDs remain valid"),
            "crystal-forge-source-v1-86c02858f73f0650c86a567a8c240378e31f2979"
        );
    }

    #[tokio::test]
    async fn mirror_fetch_ignores_ambient_config_and_persists_no_remote_url() {
        let temporary = tempfile::tempdir().expect("temporary directory should exist");
        let repository = temporary.path().join("repository");
        std::fs::create_dir(&repository).expect("repository should exist");
        assert!(run_test_git(&repository, &["init", "-q"]).status.success());
        assert!(
            run_test_git(&repository, &["config", "user.name", "Fixture"])
                .status
                .success()
        );
        assert!(
            run_test_git(
                &repository,
                &["config", "user.email", "fixture@example.invalid"]
            )
            .status
            .success()
        );
        std::fs::write(repository.join("flake.lock"), "{}").expect("fixture lock should write");
        assert!(run_test_git(&repository, &["add", "."]).status.success());
        assert!(
            run_test_git(&repository, &["commit", "-qm", "fixture"])
                .status
                .success()
        );
        let revision = run_test_git(&repository, &["rev-parse", "HEAD"]);
        assert!(revision.status.success());
        let revision = String::from_utf8(revision.stdout)
            .expect("revision should be UTF-8")
            .trim()
            .to_string();

        let mirror = temporary.path().join("mirror.git");
        ensure_mirror_has_commit(
            &mirror,
            repository
                .to_str()
                .expect("repository path should be UTF-8"),
            &revision,
            None,
        )
        .await
        .expect("explicit local fetch should succeed");

        let config = std::fs::read_to_string(mirror.join("config"))
            .expect("bare repository config should be readable");
        assert!(!config.contains(repository.to_str().expect("path should be UTF-8")));
        assert!(!config.contains("remote \"origin\""));
    }

    #[test]
    fn repository_lock_path_is_shared_across_processes() {
        let root = Path::new("/var/lib/crystal-forge/source-archives");
        assert_eq!(
            lock_path(root, "repo-abc"),
            root.join("locks/repo-abc.lock")
        );
    }

    #[tokio::test]
    async fn missing_publication_is_typed_but_digest_corruption_is_deterministic() {
        let root = tempfile::tempdir().expect("temporary source root should exist");
        let repo_url = "https://example.invalid/repository.git";
        let commit_hash = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

        let missing = lookup_published_source(root.path(), repo_url, commit_hash)
            .await
            .expect_err("missing publication should fail");
        assert_eq!(missing.class, MaterializationFailureClass::NotPublished);

        let mirror_id = source_mirror_id(repo_url);
        let artifact = artifact_path(root.path(), &mirror_id, commit_hash);
        let identity = identity_path(root.path(), &mirror_id, commit_hash);
        tokio::fs::create_dir_all(
            artifact
                .parent()
                .expect("artifact path should have a parent"),
        )
        .await
        .expect("artifact directory should exist");
        tokio::fs::create_dir_all(
            identity
                .parent()
                .expect("identity path should have a parent"),
        )
        .await
        .expect("identity directory should exist");
        tokio::fs::write(&artifact, b"corrupt artifact")
            .await
            .expect("artifact should write");
        let published_identity = ImmutableSourceIdentity {
            schema_version: VERIFIED_SOURCE_MATERIALIZATION_SCHEMA_VERSION,
            store_name: source_store_name(commit_hash).expect("commit should be valid"),
            nar_hash: "sha256-source".to_string(),
            lock_hash: "lock-hash".to_string(),
            artifact_format_version: VERIFIED_SOURCE_ARTIFACT_FORMAT_VERSION,
            artifact_sha256: "0".repeat(64),
            artifact_size: b"corrupt artifact".len() as u64,
            server_store_path: "/nix/store/source".to_string(),
        };
        tokio::fs::write(
            &identity,
            serde_json::to_vec(&published_identity).expect("identity should serialize"),
        )
        .await
        .expect("identity should write");

        let corrupted = lookup_published_source(root.path(), repo_url, commit_hash)
            .await
            .expect_err("digest corruption should fail");
        assert_eq!(corrupted.class, MaterializationFailureClass::Deterministic);
    }

    #[test]
    fn materialization_v1_rejects_sha256_object_ids_before_git_work() {
        let error = source_store_name(&"a".repeat(64))
            .expect_err("schema version 1 must reject SHA-256 repositories");
        assert_eq!(
            error.class,
            MaterializationFailureClass::UnsupportedObjectFormat
        );
        assert!(
            error
                .to_string()
                .contains("SHA-256 object IDs are unsupported")
        );
    }
}

use crate::config;
use crate::models::commits::Commit;
use crate::queries::commits::{
    flake_has_commits, flake_last_commit, insert_commit, insert_commit_with_metadata,
};
use anyhow::{Context, Result, bail};
use sqlx::PgPool;
use std::collections::HashMap;
use tokio::time::{Duration, sleep, timeout};
use tracing::{debug, info, warn};

const GIT_METADATA_TIMEOUT: Duration = Duration::from_secs(10);
const GIT_PROBE_TIMEOUT: Duration = Duration::from_secs(10);
const NIX_CONFIG_EVAL_TIMEOUT: Duration = Duration::from_secs(60);
const INIT_COMMIT_RETRY_ATTEMPTS: usize = 5;
const INIT_COMMIT_RETRY_DELAY: Duration = Duration::from_secs(1);

async fn fetch_and_insert_recent_commits_with_retry(
    pool: &PgPool,
    repo_url: &str,
    branch: &str,
    limit: Option<usize>,
) -> Result<Vec<String>> {
    let mut last_err: Option<anyhow::Error> = None;

    for attempt in 1..=INIT_COMMIT_RETRY_ATTEMPTS {
        match fetch_and_insert_recent_commits(pool, repo_url, branch, limit).await {
            Ok(commits) => return Ok(commits),
            Err(err) => {
                last_err = Some(err);

                if attempt < INIT_COMMIT_RETRY_ATTEMPTS {
                    warn!(
                        "⚠️ Commit initialization attempt {}/{} failed for {} (branch {}), retrying in {:?}",
                        attempt,
                        INIT_COMMIT_RETRY_ATTEMPTS,
                        repo_url,
                        branch,
                        INIT_COMMIT_RETRY_DELAY
                    );
                    sleep(INIT_COMMIT_RETRY_DELAY).await;
                }
            }
        }
    }

    Err(last_err.unwrap_or_else(|| anyhow::anyhow!("commit initialization failed")))
}

#[derive(Debug, Clone)]
pub struct GitCommitMetadata {
    pub message: String,
    pub author_name: String,
    pub author_email: Option<String>,
}

/// Fetches the latest commit from a git repository and inserts it into the database
pub async fn fetch_and_insert_latest_commit(
    pool: &PgPool,
    repo_url: &str,
    branch: &str,
) -> Result<Option<String>> {
    let commits = get_commits_with_timestamps(repo_url, branch, Some(1), None).await?;

    let (commit_hash, timestamp) = commits
        .into_iter()
        .next()
        .context("No commits found in repository")?;

    insert_commit(pool, &commit_hash, repo_url, timestamp).await?;

    info!(
        "✅ Inserted latest commit {} for repo {}",
        commit_hash, repo_url
    );
    Ok(Some(commit_hash))
}

/// Fetch up to N recent commits from a git repository and insert them into the database
pub async fn fetch_and_insert_recent_commits(
    pool: &PgPool,
    repo_url: &str,
    branch: &str,
    limit: Option<usize>,
) -> Result<Vec<String>> {
    let commits = get_commits_with_full_metadata(repo_url, branch, limit, None).await?;

    let mut inserted = Vec::new();
    for commit_data in commits {
        if let Err(e) = insert_commit_with_metadata(
            pool,
            &commit_data.hash,
            repo_url,
            commit_data.timestamp,
            Some(&commit_data.message),
            Some(&commit_data.author),
        )
        .await
        {
            warn!("Failed to insert commit {}: {}", commit_data.hash, e);
        } else {
            inserted.push(commit_data.hash);
        }
    }

    Ok(inserted)
}

// TODO: update this to get the last N commits for each flake if we are starting for the first time
/// Initialize commits for all watched flakes that don't have any commits yet
/// This is meant to run once when the server first starts
pub async fn initialize_flake_commits(
    pool: &PgPool,
    watched_flakes: &[crate::config::WatchedFlake],
) -> Result<()> {
    info!(
        "🔄 Initializing commits for {} watched flakes",
        watched_flakes.len()
    );

    for flake in watched_flakes {
        if !flake.auto_poll {
            debug!("⏭️ Skipping {} (auto_poll = false)", flake.name);
            continue;
        }

        // Check if this flake already has commits
        match flake_has_commits(pool, &flake.repo_url).await {
            Ok(true) => {
                debug!("⏭️ Skipping {} (already has commits)", flake.name);
                continue;
            }
            Ok(false) => {
                info!("🔗 Initializing commits for flake: {}", flake.name);
            }
            Err(e) => {
                warn!("❌ Failed to check commits for {}: {}", flake.name, e);
                continue;
            }
        }

        match fetch_and_insert_recent_commits_with_retry(
            pool,
            &flake.repo_url,
            &flake.branch(),
            Some(flake.initial_commit_depth),
        )
        .await
        {
            Ok(commits) => {
                info!(
                    "✅ Successfully initialized {} commits for {} on branch {}",
                    commits.len(),
                    flake.name,
                    flake.branch()
                );
            }
            Err(e) => {
                warn!(
                    "❌ Failed to initialize commits for {}: {} on branch {}",
                    flake.name,
                    e,
                    flake.branch()
                );
            }
        }
    }

    Ok(())
}

/// Sync commits for all watched flakes that have auto_poll enabled (for regular polling)
pub async fn sync_all_watched_flakes_commits(
    pool: &PgPool,
    watched_flakes: &[config::WatchedFlake],
) -> Result<usize> {
    info!(
        "🔄 Syncing commits for {} watched flakes",
        watched_flakes.len()
    );

    let mut total_inserted = 0;

    for flake in watched_flakes {
        if !flake.auto_poll {
            debug!("⭐️ Skipping {} (auto_poll = false)", flake.name);
            continue;
        }

        info!("🔗 Syncing commits for flake: {}", flake.name);

        // Check if flake has commits first
        match flake_has_commits(pool, &flake.repo_url).await {
            Ok(true) => {
                // Has commits, do incremental sync
                match flake_last_commit(pool, &flake.repo_url).await {
                    Ok(last_commit) => {
                        match fetch_and_insert_commits_since(
                            pool,
                            &flake.repo_url,
                            &flake.branch(),
                            &last_commit,
                        )
                        .await
                        {
                            Ok(new_commits) => {
                                let count = new_commits.len();
                                total_inserted += count;
                                if count > 0 {
                                    info!(
                                        "✅ Found {} new commits for {}",
                                        count,
                                        flake.name
                                    );
                                } else {
                                    debug!("📍 No new commits for {}", flake.name);
                                }
                            }
                            Err(e) => {
                                warn!("⚠️ Failed to sync new commits for {}: {}", flake.name, e);
                            }
                        }
                    }
                    Err(e) => {
                        warn!("⚠️ Failed to get last commit for {}: {}", flake.name, e);
                    }
                }
            }
            Ok(false) => {
                // No commits, initialize
                info!("🔄 Initializing commits for flake: {}", flake.name);
                match fetch_and_insert_recent_commits_with_retry(
                    pool,
                    &flake.repo_url,
                    &flake.branch(),
                    Some(flake.initial_commit_depth),
                )
                .await
                {
                    Ok(commits) => {
                        let count = commits.len();
                        total_inserted += count;
                        info!(
                            "✅ Successfully initialized {} commits for {}",
                            count,
                            flake.name
                        );
                    }
                    Err(e) => {
                        warn!("⚠️ Failed to initialize commits for {}: {}", flake.name, e);
                    }
                }
            }
            Err(e) => {
                warn!("⚠️ Failed to check commits for {}: {}", flake.name, e);
            }
        }
    }

    Ok(total_inserted)
}

/// Sync commits for a single flake repository URL.
///
/// Returns the number of newly inserted commits.
pub async fn sync_commits_for_repo(pool: &PgPool, repo_url: &str, branch: &str) -> Result<usize> {
    match flake_has_commits(pool, repo_url).await {
        Ok(true) => {
            let last_commit = flake_last_commit(pool, repo_url)
                .await
                .with_context(|| format!("Failed to load last commit for {repo_url}"))?;
            let inserted = fetch_and_insert_commits_since(pool, repo_url, branch, &last_commit)
                .await
                .with_context(|| {
                    format!("Failed to sync commits since last known hash for {repo_url}")
                })?;
            Ok(inserted.len())
        }
        Ok(false) => {
            let inserted = fetch_and_insert_recent_commits(pool, repo_url, branch, Some(10))
                .await
                .with_context(|| format!("Failed to initialize commits for {repo_url}"))?;
            Ok(inserted.len())
        }
        Err(e) => Err(e).with_context(|| format!("Failed to inspect commit state for {repo_url}")),
    }
}

/// Resolve the remote default branch name for a repository.
pub async fn infer_default_branch(repo_url: &str) -> Result<String> {
    let git_url = normalize_repo_url_for_git(repo_url);
    let output = timeout(
        GIT_PROBE_TIMEOUT,
        tokio::process::Command::new("git")
            .args(["ls-remote", "--symref", &git_url, "HEAD"])
            .output(),
    )
    .await
    .with_context(|| format!("Timed out probing default branch for {repo_url}"))?
    .with_context(|| format!("Failed to probe default branch for {repo_url}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("Git ls-remote failed for {repo_url}: {}", stderr.trim());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        if let Some(target) = line
            .strip_prefix("ref: refs/heads/")
            .and_then(|value| value.split('\t').next())
        {
            let branch = target.trim();
            if !branch.is_empty() {
                return Ok(branch.to_string());
            }
        }
    }

    bail!("Unable to determine default branch for {repo_url}")
}

/// Check whether a specific branch exists on the remote repository.
pub async fn branch_exists(repo_url: &str, branch: &str) -> Result<bool> {
    let git_url = normalize_repo_url_for_git(repo_url);
    let refspec = format!("refs/heads/{branch}");

    let output = timeout(
        GIT_PROBE_TIMEOUT,
        tokio::process::Command::new("git")
            .args(["ls-remote", &git_url, &refspec])
            .output(),
    )
    .await
    .with_context(|| format!("Timed out probing branch {branch} for {repo_url}"))?
    .with_context(|| format!("Failed to probe branch {branch} for {repo_url}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!(
            "Git ls-remote failed for {repo_url} on {branch}: {}",
            stderr.trim()
        );
    }

    Ok(!String::from_utf8_lossy(&output.stdout).trim().is_empty())
}

fn normalize_repo_url_for_git(repo_url: &str) -> String {
    let base_url = if let Some(stripped) = repo_url.strip_prefix("git+") {
        stripped
    } else if repo_url.starts_with("github:") {
        let repo_path = repo_url.strip_prefix("github:").unwrap();
        return format!("https://github.com/{}", repo_path);
    } else if repo_url.starts_with("gitlab:") {
        let repo_path = repo_url.strip_prefix("gitlab:").unwrap();
        return format!("https://gitlab.com/{}", repo_path);
    } else {
        repo_url
    };

    // Strip query parameters for git operations
    if let Some(question_mark_pos) = base_url.find('?') {
        base_url[..question_mark_pos].to_string()
    } else {
        base_url.to_string()
    }
}

/// Get commits with timestamps, optionally since a specific commit
/// Commit data fetched from git log
#[derive(Debug, Clone)]
struct CommitData {
    hash: String,
    timestamp: chrono::DateTime<chrono::Utc>,
    message: String,
    author: String,
}

async fn get_commits_with_full_metadata(
    repo_url: &str,
    branch: &str,
    limit: Option<usize>,
    since_commit: Option<&str>,
) -> Result<Vec<CommitData>> {
    let git_url = normalize_repo_url_for_git(repo_url);
    let temp_dir = tempfile::tempdir().context("Failed to create temporary directory")?;
    let clone_path = temp_dir.path();

    // Clone
    let depth = limit.unwrap_or(10).to_string();
    let clone_output = tokio::process::Command::new("git")
        .args(&[
            "clone",
            "--depth",
            &depth,
            "--branch",
            branch,
            "--single-branch",
            &git_url,
            ".",
        ])
        .current_dir(clone_path)
        .output()
        .await?;

    if !clone_output.status.success() {
        let stderr = String::from_utf8_lossy(&clone_output.stderr);
        bail!("Git clone failed for {}: {}", repo_url, stderr);
    }

    // Build git log args with format: hash|timestamp|subject|author
    // Using %x1E as field separator (ASCII record separator) to handle multi-line messages
    let mut args = vec!["log", "--format=%H%x1E%cI%x1E%s%x1E%aN"];

    // Add range if since_commit provided
    let range;
    let max_count;

    if let Some(since) = since_commit {
        range = format!("{}..HEAD", since);
        args.push(&range);
    } else if let Some(lim) = limit {
        max_count = format!("--max-count={}", lim);
        args.push(&max_count);
    }

    let log_output = tokio::process::Command::new("git")
        .args(&args)
        .current_dir(clone_path)
        .output()
        .await
        .context("Failed to spawn git log")?;

    let mut log_output = log_output;

    if !log_output.status.success() {
        let stderr = String::from_utf8_lossy(&log_output.stderr);

        if since_commit.is_some() && stderr.contains("Invalid revision range") {
            let fetch_output = tokio::process::Command::new("git")
                .args(&["fetch", "--unshallow", "--tags", "origin", branch])
                .current_dir(clone_path)
                .output()
                .await
                .context("Failed to spawn git fetch --unshallow")?;

            if !fetch_output.status.success() {
                let fetch_stderr = String::from_utf8_lossy(&fetch_output.stderr);
                bail!("git fetch failed: {}", fetch_stderr.trim());
            }

            log_output = tokio::process::Command::new("git")
                .args(&args)
                .current_dir(clone_path)
                .output()
                .await
                .context("Failed to spawn git log (retry)")?;
        }
    }

    if !log_output.status.success() {
        let stderr = String::from_utf8_lossy(&log_output.stderr);
        bail!("git log failed: {}", stderr.trim());
    }

    let stdout = String::from_utf8(log_output.stdout)?;
    let commits: Result<Vec<_>> = stdout
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let parts: Vec<&str> = line.split('\x1E').collect();
            if parts.len() != 4 {
                bail!("Invalid git log format (expected 4 fields): {}", line);
            }
            let hash = parts[0].trim().to_string();
            let timestamp = chrono::DateTime::parse_from_rfc3339(parts[1].trim())
                .context("Failed to parse timestamp")?
                .with_timezone(&chrono::Utc);
            let message = parts[2].trim().to_string();
            let author = parts[3].trim().to_string();
            Ok(CommitData {
                hash,
                timestamp,
                message,
                author,
            })
        })
        .collect();

    commits
}

/// Legacy function for backward compatibility - returns only hash and timestamp
async fn get_commits_with_timestamps(
    repo_url: &str,
    branch: &str,
    limit: Option<usize>,
    since_commit: Option<&str>,
) -> Result<Vec<(String, chrono::DateTime<chrono::Utc>)>> {
    let commits = get_commits_with_full_metadata(repo_url, branch, limit, since_commit).await?;
    Ok(commits.into_iter().map(|c| (c.hash, c.timestamp)).collect())
}

/// Fetch and insert all new commits since a given commit hash
pub async fn fetch_and_insert_commits_since(
    pool: &PgPool,
    repo_url: &str,
    branch: &str,
    since_commit: &Commit,
) -> Result<Vec<String>> {
    let commits = get_commits_with_full_metadata(
        repo_url,
        branch,
        Some(50),
        Some(&since_commit.git_commit_hash),
    )
    .await?;

    if commits.is_empty() {
        debug!(
            "No new commits found since {} for {}",
            since_commit, repo_url
        );
        return Ok(Vec::new());
    }

    let mut inserted = Vec::new();
    // Insert in reverse (oldest first) for chronological order
    for commit_data in commits.into_iter().rev() {
        if let Err(e) = insert_commit_with_metadata(
            pool,
            &commit_data.hash,
            repo_url,
            commit_data.timestamp,
            Some(&commit_data.message),
            Some(&commit_data.author),
        )
        .await
        {
            warn!("Failed to insert commit {}: {}", commit_data.hash, e);
        } else {
            debug!("✅ Inserted commit {} for {}", commit_data.hash, repo_url);
            inserted.push(commit_data.hash);
        }
    }

    info!(
        "✅ Inserted {} new commits for {}",
        inserted.len(),
        repo_url
    );
    Ok(inserted)
}

/// Resolve commit subject/author metadata for specific hashes.
///
/// Best effort: hashes that cannot be resolved are skipped.
pub async fn get_commit_metadata(
    repo_url: &str,
    commit_hashes: &[String],
) -> Result<HashMap<String, GitCommitMetadata>> {
    if commit_hashes.is_empty() {
        return Ok(HashMap::new());
    }

    let git_url = normalize_repo_url_for_git(repo_url);
    let temp_dir = tempfile::tempdir().context("Failed to create temporary directory")?;
    let clone_path = temp_dir.path();

    let clone_output = timeout(
        GIT_METADATA_TIMEOUT,
        tokio::process::Command::new("git")
            .args([
                "clone",
                "--depth",
                "200",
                "--filter=blob:none",
                &git_url,
                ".",
            ])
            .current_dir(clone_path)
            .output(),
    )
    .await
    .with_context(|| format!("Timed out cloning repo for metadata: {repo_url}"))?
    .with_context(|| format!("Failed to clone repo for metadata: {repo_url}"))?;

    if !clone_output.status.success() {
        let stderr = String::from_utf8_lossy(&clone_output.stderr);
        bail!("Git clone failed for {}: {}", repo_url, stderr.trim());
    }

    let prefetch = timeout(
        GIT_METADATA_TIMEOUT,
        tokio::process::Command::new("git")
            .args(["fetch", "--quiet", "--depth", "200", "origin"])
            .current_dir(clone_path)
            .output(),
    )
    .await;
    match prefetch {
        Ok(Ok(output)) if output.status.success() => {}
        Ok(Ok(output)) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            warn!(
                "Best-effort metadata prefetch failed for {}: {}",
                repo_url,
                stderr.trim()
            );
        }
        Ok(Err(err)) => {
            warn!(
                "Best-effort metadata prefetch failed for {}: {}",
                repo_url, err
            );
        }
        Err(_) => {
            warn!("Best-effort metadata prefetch timed out for {}", repo_url);
        }
    }

    let mut metadata = HashMap::new();
    for hash in commit_hashes {
        match load_commit_metadata(clone_path, hash).await {
            Ok(value) => {
                metadata.insert(hash.clone(), value);
            }
            Err(err) => {
                warn!("Failed to load git metadata for {}: {}", hash, err);
            }
        }
    }

    Ok(metadata)
}

/// Resolve `nixosConfigurations` names for specific commit hashes.
///
/// Best effort: commits that fail to evaluate are skipped.
/// Processes commits sequentially to avoid overwhelming nix eval.
pub async fn get_commit_nixos_configurations(
    repo_url: &str,
    commit_hashes: &[String],
) -> HashMap<String, Vec<String>> {
    let mut results = HashMap::new();

    // Limit to first 5 commits to avoid timeout cascade
    let limited_hashes = if commit_hashes.len() > 5 {
        warn!(
            "Limiting nixosConfigurations hydration to 5 commits (requested {})",
            commit_hashes.len()
        );
        &commit_hashes[..5]
    } else {
        commit_hashes
    };

    for hash in limited_hashes {
        match load_commit_nixos_configurations(repo_url, hash).await {
            Ok(configs) => {
                results.insert(hash.clone(), configs);
            }
            Err(err) => {
                warn!(
                    "Failed to resolve nixosConfigurations for {} @ {}: {}",
                    repo_url, hash, err
                );
            }
        }
    }

    results
}

/// Resolve changed file paths for specific commit hashes.
///
/// Best effort: commits that cannot be resolved are skipped.
pub async fn get_commit_changed_files(
    repo_url: &str,
    commit_hashes: &[String],
) -> Result<HashMap<String, Vec<String>>> {
    if commit_hashes.is_empty() {
        return Ok(HashMap::new());
    }

    let git_url = normalize_repo_url_for_git(repo_url);
    let temp_dir = tempfile::tempdir().context("Failed to create temporary directory")?;
    let clone_path = temp_dir.path();

    let clone_output = timeout(
        GIT_METADATA_TIMEOUT,
        tokio::process::Command::new("git")
            .args([
                "clone",
                "--depth",
                "200",
                "--filter=blob:none",
                &git_url,
                ".",
            ])
            .current_dir(clone_path)
            .output(),
    )
    .await
    .with_context(|| format!("Timed out cloning repo for changed files: {repo_url}"))?
    .with_context(|| format!("Failed to clone repo for changed files: {repo_url}"))?;

    if !clone_output.status.success() {
        let stderr = String::from_utf8_lossy(&clone_output.stderr);
        bail!("Git clone failed for {}: {}", repo_url, stderr.trim());
    }

    let mut changed = HashMap::new();
    for hash in commit_hashes {
        match load_commit_changed_files(clone_path, hash).await {
            Ok(files) => {
                changed.insert(hash.clone(), files);
            }
            Err(err) => {
                warn!("Failed to load changed files for {}: {}", hash, err);
            }
        }
    }

    Ok(changed)
}

async fn load_commit_nixos_configurations(
    repo_url: &str,
    commit_hash: &str,
) -> Result<Vec<String>> {
    let flake_ref = build_flake_reference(repo_url, commit_hash);
    let flake_target = format!("{flake_ref}#nixosConfigurations");

    let output = timeout(
        NIX_CONFIG_EVAL_TIMEOUT,
        tokio::process::Command::new("nix")
            .args([
                "eval",
                "--json",
                "--apply",
                "builtins.attrNames",
                flake_target.as_str(),
            ])
            .output(),
    )
    .await
    .with_context(|| format!("Timed out evaluating nixosConfigurations for {commit_hash}"))?
    .with_context(|| format!("Failed to evaluate nixosConfigurations for {commit_hash}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("nix eval failed for {}: {}", commit_hash, stderr.trim());
    }

    let mut names: Vec<String> = serde_json::from_slice(&output.stdout)
        .with_context(|| format!("Failed to parse nixosConfigurations JSON for {commit_hash}"))?;
    names.sort();
    names.dedup();
    Ok(names)
}

fn build_flake_reference(repo_url: &str, commit_hash: &str) -> String {
    if repo_url.starts_with("git+") {
        if repo_url.contains("?rev=") {
            repo_url.to_string()
        } else {
            format!("{}?rev={}", repo_url, commit_hash)
        }
    } else {
        let separator = if repo_url.contains('?') { "&" } else { "?" };
        format!("git+{}{separator}rev={}", repo_url, commit_hash)
    }
}

async fn load_commit_changed_files(
    clone_path: &std::path::Path,
    commit_hash: &str,
) -> Result<Vec<String>> {
    let output = timeout(
        GIT_METADATA_TIMEOUT,
        tokio::process::Command::new("git")
            .args(["show", "--pretty=format:", "--name-only", commit_hash])
            .current_dir(clone_path)
            .output(),
    )
    .await
    .with_context(|| format!("Timed out loading changed files for commit {commit_hash}"))?
    .with_context(|| format!("Failed to load changed files for commit {commit_hash}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!(
            "git show --name-only failed for {}: {}",
            commit_hash,
            stderr.trim()
        );
    }

    let mut files: Vec<String> = String::from_utf8(output.stdout)?
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToOwned::to_owned)
        .collect();
    files.sort();
    files.dedup();
    Ok(files)
}

async fn load_commit_metadata(
    clone_path: &std::path::Path,
    commit_hash: &str,
) -> Result<GitCommitMetadata> {
    let output = timeout(
        GIT_METADATA_TIMEOUT,
        tokio::process::Command::new("git")
            .args(["show", "-s", "--format=%H%x1f%s%x1f%an%x1f%ae", commit_hash])
            .current_dir(clone_path)
            .output(),
    )
    .await
    .with_context(|| format!("Timed out loading metadata for commit {commit_hash}"))?
    .with_context(|| format!("Failed to load metadata for commit {commit_hash}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("git show failed for {}: {}", commit_hash, stderr.trim());
    }

    let stdout = String::from_utf8(output.stdout)?;
    let line = stdout
        .lines()
        .find(|value| !value.trim().is_empty())
        .context("git show returned empty output")?;

    let mut parts = line.split('\u{1f}');
    let _hash = parts.next().context("Missing hash")?;
    let message = parts.next().context("Missing commit subject")?.trim();
    let author_name = parts.next().context("Missing author name")?.trim();
    let author_email = parts.next().unwrap_or("").trim();

    Ok(GitCommitMetadata {
        message: message.to_string(),
        author_name: author_name.to_string(),
        author_email: if author_email.is_empty() {
            None
        } else {
            Some(author_email.to_string())
        },
    })
}

/// Get the git diff for a specific commit.
/// Returns the full unified diff output from `git show`.
/// Tries multiple common branch names if the specified branch doesn't work.
/// Get the git diff for a specific commit.
/// Returns the full unified diff output from `git show`.
/// Tries multiple common branch names if the specified branch doesn't work.
pub async fn get_commit_diff(repo_url: &str, branch: &str, commit_hash: &str) -> Result<String> {
    let git_url = normalize_repo_url_for_git(repo_url);
    let temp_dir = tempfile::tempdir().context("Failed to create temporary directory")?;
    let clone_path = temp_dir.path();

    // Try the specified branch first, then fall back to common branch names
    let branches_to_try = vec![
        branch.to_string(),
        "main".to_string(),
        "master".to_string(),
        "HEAD".to_string(),
    ];

    for branch_to_try in branches_to_try.iter() {
        let result =
            try_get_diff_for_branch(&git_url, clone_path, branch_to_try, commit_hash).await;
        if let Ok(diff) = result {
            return Ok(diff);
        }
    }

    // If all branches fail, return an error
    let branch_list = branches_to_try.join(", ");
    bail!(
        "Could not find commit {} in any branch (tried: {})",
        commit_hash,
        branch_list
    )
}

async fn try_get_diff_for_branch(
    git_url: &str,
    clone_path: &std::path::Path,
    branch: &str,
    commit_hash: &str,
) -> Result<String> {
    // Clone with minimal depth since we only need one specific commit
    let clone_output = tokio::process::Command::new("git")
        .args(&[
            "clone",
            "--depth",
            "50", // Get enough depth to potentially find the commit
            "--branch",
            branch,
            "--single-branch",
            &git_url,
            ".",
        ])
        .current_dir(clone_path)
        .output()
        .await?;

    if !clone_output.status.success() {
        // Clone failed for this branch, try next one
        return Err(anyhow::anyhow!("Branch {} not found", branch));
    }

    // Try to get the diff for the commit
    let show_output = tokio::process::Command::new("git")
        .args(&[
            "show",
            "--format=", // Don't show commit message/metadata, just diff
            commit_hash,
        ])
        .current_dir(clone_path)
        .output()
        .await?;

    if !show_output.status.success() {
        // If the commit isn't in the shallow clone, try to fetch it
        let fetch_output = tokio::process::Command::new("git")
            .args(&["fetch", "origin", commit_hash])
            .current_dir(clone_path)
            .output()
            .await?;

        if !fetch_output.status.success() {
            let stderr = String::from_utf8_lossy(&show_output.stderr);
            bail!("Failed to fetch commit {}: {}", commit_hash, stderr);
        }

        // Retry git show
        let retry_output = tokio::process::Command::new("git")
            .args(&["show", "--format=", commit_hash])
            .current_dir(clone_path)
            .output()
            .await?;

        if !retry_output.status.success() {
            let stderr = String::from_utf8_lossy(&retry_output.stderr);
            bail!("git show failed for {}: {}", commit_hash, stderr);
        }

        return Ok(String::from_utf8_lossy(&retry_output.stdout).to_string());
    }

    Ok(String::from_utf8_lossy(&show_output.stdout).to_string())
}

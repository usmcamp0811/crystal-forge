use crate::models::config;
use crate::queries::commits::{flake_has_commits, flake_last_commit, insert_commit};
use anyhow::{Context, Result};
use sqlx::PgPool;
use tracing::{debug, info, warn};

/// Fetches the latest commit from a git repository and inserts it into the database
pub async fn fetch_and_insert_latest_commit(
    pool: &PgPool,
    repo_url: &str,
    branch: &str,
) -> Result<Option<String>> {
    let commit_hash = get_latest_commit_hash(repo_url, branch).await?;

    insert_commit(pool, &commit_hash, repo_url).await?;
    info!(
        "✅ Inserted latest commit {} for repo {}",
        commit_hash, repo_url
    );
    Ok(Some(commit_hash))
}

/// Get the past N commit hashes from a specific branch in a git repository
async fn get_recent_commit_hashes(
    repo_url: &str,
    branch: &str,
    limit: usize,
) -> Result<Vec<String>> {
    let git_url = normalize_repo_url_for_git(repo_url);

    // Create a temporary directory for the shallow clone
    let temp_dir = tempfile::tempdir().with_context(|| "Failed to create temporary directory")?;
    let clone_path = temp_dir.path();

    // Perform shallow clone with the specified depth
    let clone_output = tokio::process::Command::new("git")
        .args(&[
            "clone",
            "--depth",
            &limit.to_string(),
            "--branch",
            branch,
            "--single-branch",
            &git_url,
            ".",
        ])
        .current_dir(clone_path)
        .output()
        .await
        .with_context(|| {
            format!(
                "Failed to spawn git clone for '{}' with depth {}",
                repo_url, limit
            )
        })?;

    if !clone_output.status.success() {
        let stderr = String::from_utf8_lossy(&clone_output.stderr);
        let exit_code = clone_output.status.code().unwrap_or(-1);

        return Err(anyhow::anyhow!(
            "git clone failed for repository '{}' branch '{}' (exit code: {})\nError output: {}",
            repo_url,
            branch,
            exit_code,
            stderr.trim()
        ));
    }

    // Now get the commit history using git log
    let log_output = tokio::process::Command::new("git")
        .args(&[
            "log",
            "--format=%H", // Only output commit hashes
            &format!("--max-count={}", limit),
        ])
        .current_dir(clone_path)
        .output()
        .await
        .with_context(|| {
            format!(
                "Failed to spawn git log in cloned repository for '{}'",
                repo_url
            )
        })?;

    if !log_output.status.success() {
        let stderr = String::from_utf8_lossy(&log_output.stderr);
        let exit_code = log_output.status.code().unwrap_or(-1);

        return Err(anyhow::anyhow!(
            "git log failed in cloned repository for '{}' branch '{}' (exit code: {})\nError output: {}",
            repo_url,
            branch,
            exit_code,
            stderr.trim()
        ));
    }

    let stdout = String::from_utf8(log_output.stdout)?;
    let commits: Vec<String> = stdout
        .lines()
        .map(|line| line.trim().to_string())
        .filter(|line| !line.is_empty())
        .collect();

    if commits.is_empty() {
        return Err(anyhow::anyhow!(
            "No commits found in repository '{}' for branch '{}'",
            repo_url,
            branch
        ));
    }

    // Temporary directory is automatically cleaned up when temp_dir goes out of scope
    Ok(commits)
}

/// Get the latest commit hash from a specific branch in a git repository
async fn get_latest_commit_hash(repo_url: &str, branch: &str) -> Result<String> {
    let commits = get_recent_commit_hashes(repo_url, branch, 1).await?;
    Ok(commits.into_iter().next().unwrap()) // Safe because we know there's at least 1
}

/// Fetch up to N recent commits from a git repository and insert them into the database
pub async fn fetch_and_insert_recent_commits(
    pool: &PgPool,
    repo_url: &str,
    branch: &str,
    limit: Option<usize>,
) -> Result<Vec<String>> {
    let limit = limit.unwrap_or(10); // Default to 10 if not specified
    let commit_hashes = get_recent_commit_hashes(repo_url, branch, limit).await?;

    let mut inserted_commits = Vec::new();

    // Insert commits in reverse order (oldest first) so they're in chronological order
    for commit_hash in commit_hashes.into_iter().rev() {
        match insert_commit(pool, &commit_hash, repo_url).await {
            Ok(_) => {
                debug!("✅ Inserted commit {} for repo {}", commit_hash, repo_url);
                inserted_commits.push(commit_hash);
            }
            Err(e) => {
                warn!(
                    "❌ Failed to insert commit {} for repo {}: {}",
                    commit_hash, repo_url, e
                );
            }
        }
    }

    info!(
        "✅ Inserted {} commits for repo {}",
        inserted_commits.len(),
        repo_url
    );
    Ok(inserted_commits)
}

// TODO: update this to get the last N commits for each flake if we are starting for the first time
/// Initialize commits for all watched flakes that don't have any commits yet
/// This is meant to run once when the server first starts
pub async fn initialize_flake_commits(
    pool: &PgPool,
    watched_flakes: &[crate::models::config::WatchedFlake],
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

        match fetch_and_insert_recent_commits(
            pool,
            &flake.repo_url,
            &flake.branch,
            Some(flake.initial_commit_depth),
        )
        .await
        {
            Ok(commits) => {
                info!(
                    "✅ Successfully initialized {} commits for {} on branch {}",
                    commits.len(),
                    flake.name,
                    flake.branch
                );
            }
            Err(e) => {
                warn!(
                    "❌ Failed to initialize commits for {}: {} on branch {}",
                    flake.name, e, flake.branch
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
) -> Result<()> {
    info!(
        "🔄 Syncing commits for {} watched flakes",
        watched_flakes.len()
    );

    for flake in watched_flakes {
        if !flake.auto_poll {
            debug!("⏭️ Skipping {} (auto_poll = false)", flake.name);
            continue;
        }

        info!("🔗 Syncing commits for flake: {}", flake.name);

        match fetch_and_insert_commits_since(pool, &flake.repo_url, &flake.branch).await {
            Ok(Some(commit_hash)) => {
                info!(
                    "✅ Successfully synced commit {} for {}",
                    commit_hash, flake.name
                );
            }
            Ok(None) => {
                warn!("⚠️ No commits found for {}", flake.name);
            }
            Err(e) => {
                warn!("❌ Failed to sync commits for {}: {}", flake.name, e);
            }
        }
    }

    Ok(())
}

fn normalize_repo_url_for_git(repo_url: &str) -> String {
    // Handle different Nix flake URL formats
    if let Some(stripped) = repo_url.strip_prefix("git+") {
        // Remove git+ prefix
        stripped.to_string()
    } else if repo_url.starts_with("github:") {
        // Convert github: to https://
        let repo_path = repo_url.strip_prefix("github:").unwrap();
        format!("https://github.com/{}", repo_path)
    } else if repo_url.starts_with("gitlab:") {
        // Convert gitlab: to https://
        let repo_path = repo_url.strip_prefix("gitlab:").unwrap();
        format!("https://gitlab.com/{}", repo_path)
    } else {
        // Already a regular git URL
        repo_url.to_string()
    }
}

async fn run_ls_remote(repo_url: &str) -> Result<String> {
    use tokio::process::Command;

    let git_url = normalize_repo_url_for_git(repo_url);
    let output = Command::new("git")
        .args(&["ls-remote", "--heads", "--tags", &git_url])
        .output()
        .await
        .with_context(|| {
            format!(
                "Failed to spawn git process for 'git ls-remote --heads --tags {}' (normalized from '{}')",
                git_url, repo_url
            )
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let exit_code = output.status.code().unwrap_or(-1);

        return Err(anyhow::anyhow!(
            "git ls-remote failed for repository '{}' (exit code: {})\nError output: {}",
            repo_url,
            exit_code,
            stderr.trim()
        ));
    }

    Ok(String::from_utf8(output.stdout)?)
}

/// Get all new commit hashes since a given commit on a specific branch
async fn get_commits_since(
    repo_url: &str,
    branch: &str,
    since_commit: &Commit,
) -> Result<Vec<String>> {
    let git_url = normalize_repo_url_for_git(repo_url);

    let temp_dir = tempfile::tempdir()?;
    let clone_path = temp_dir.path();

    // Clone with shallow depth - should be enough for frequent polling
    tokio::process::Command::new("git")
        .args(&[
            "clone",
            "--depth",
            "20",
            "--branch",
            branch,
            "--single-branch",
            &git_url,
            ".",
        ])
        .current_dir(clone_path)
        .output()
        .await?;

    let output = tokio::process::Command::new("git")
        .args(&[
            "log",
            "--format=%H",
            &format!("{}..HEAD", since_commit.git_commit_hash),
        ])
        .current_dir(clone_path)
        .output()
        .await?;

    let commits: Vec<String> = String::from_utf8(output.stdout)?
        .lines()
        .map(|line| line.trim().to_string())
        .filter(|line| !line.is_empty())
        .collect();

    Ok(commits)
}

/// Fetch and insert all new commits since a given commit hash
pub async fn fetch_and_insert_commits_since(
    pool: &PgPool,
    repo_url: &str,
    branch: &str,
    since_commit: &Commit,
) -> Result<Vec<String>> {
    let commit_hashes = get_commits_since(repo_url, branch, since_commit).await?;

    if commit_hashes.is_empty() {
        debug!(
            "No new commits found since {} for repo {}",
            since_commit, repo_url
        );
        return Ok(Vec::new());
    }

    let mut inserted_commits = Vec::new();

    // Insert commits in reverse order (oldest first) so they're in chronological order
    for commit_hash in commit_hashes.into_iter().rev() {
        match insert_commit(pool, &commit_hash, repo_url).await {
            Ok(_) => {
                debug!("✅ Inserted commit {} for repo {}", commit_hash, repo_url);
                inserted_commits.push(commit_hash);
            }
            Err(e) => {
                warn!(
                    "❌ Failed to insert commit {} for repo {}: {}",
                    commit_hash, repo_url, e
                );
            }
        }
    }

    info!(
        "✅ Inserted {} new commits since {} for repo {}",
        inserted_commits.len(),
        since_commit,
        repo_url
    );
    Ok(inserted_commits)
}

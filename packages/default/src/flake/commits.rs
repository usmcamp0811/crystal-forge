use crate::models::config;
use crate::queries::commits::{flake_has_commits, insert_commit};
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

/// Get the latest commit hash from a specific branch in a git repository
async fn get_latest_commit_hash(repo_url: &str, branch: &str) -> Result<String> {
    let stdout = run_ls_remote(repo_url).await?;

    // Try specified branch first
    let target_ref = format!("refs/heads/{}", branch);
    if let Some(line) = stdout.lines().find(|l| l.ends_with(&target_ref)) {
        if let Some(hash) = line.split_whitespace().next() {
            return Ok(hash.to_string());
        }
    }

    // Fallback to HEAD
    if let Some(line) = stdout.lines().find(|l| l.ends_with("HEAD")) {
        if let Some(hash) = line.split_whitespace().next() {
            return Ok(hash.to_string());
        }
    }

    // Fallback to common branches
    for fallback in ["refs/heads/main", "refs/heads/master"] {
        if let Some(line) = stdout.lines().find(|l| l.ends_with(fallback)) {
            if let Some(hash) = line.split_whitespace().next() {
                return Ok(hash.to_string());
            }
        }
    }

    Err(anyhow::anyhow!(
        "No suitable commit found in ls-remote output for branch '{}'",
        branch
    ))
}

async fn get_recent_commit_hashes(repo_url: &str, branch: &str) -> Result<Vec<String>> {
    let stdout = run_ls_remote(repo_url).await?;
    let mut commits = Vec::new();

    // Try specified branch first
    let target_ref = format!("refs/heads/{}", branch);
    if let Some(line) = stdout.lines().find(|line| line.ends_with(&target_ref)) {
        if let Some(commit_hash) = line.split_whitespace().next() {
            commits.push(commit_hash.to_string());
        }
    }

    // Fallback to HEAD
    if commits.is_empty() {
        if let Some(head_line) = stdout.lines().find(|line| line.ends_with("HEAD")) {
            if let Some(commit_hash) = head_line.split_whitespace().next() {
                commits.push(commit_hash.to_string());
            }
        }
    }

    // Fallback to common defaults
    if commits.is_empty() {
        for fallback in ["refs/heads/main", "refs/heads/master"] {
            if let Some(line) = stdout.lines().find(|l| l.ends_with(fallback)) {
                if let Some(commit_hash) = line.split_whitespace().next() {
                    commits.push(commit_hash.to_string());
                    break;
                }
            }
        }
    }

    if commits.is_empty() {
        return Err(anyhow::anyhow!(
            "No commits found in repository for branch '{}'",
            branch
        ));
    }

    Ok(commits)
}

/// Fetch up to 10 recent commits from a git repository and insert them into the database
pub async fn fetch_and_insert_recent_commits(
    pool: &PgPool,
    repo_url: &str,
    branch: &str,
) -> Result<Vec<String>> {
    let commit_hashes = get_recent_commit_hashes(repo_url, branch).await?;

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

        match fetch_and_insert_recent_commits(pool, &flake.repo_url, &flake.branch).await {
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

        match fetch_and_insert_latest_commit(pool, &flake.repo_url, &flake.branch).await {
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

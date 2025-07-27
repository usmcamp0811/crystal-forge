use crate::models::config;
use crate::queries::commits::{flake_has_commits, insert_commit};
use anyhow::{Context, Result};
use sqlx::PgPool;
use tracing::{debug, info, warn};

/// Fetches the latest commit from a git repository and inserts it into the database
pub async fn fetch_and_insert_latest_commit(
    pool: &PgPool,
    repo_url: &str,
) -> Result<Option<String>> {
    let commit_hash = get_latest_commit_hash(repo_url).await?;

    insert_commit(pool, &commit_hash, repo_url).await?;
    info!(
        "✅ Inserted latest commit {} for repo {}",
        commit_hash, repo_url
    );
    Ok(Some(commit_hash))
}

/// Get the latest commit hash from any git repository
async fn get_latest_commit_hash(repo_url: &str) -> Result<String> {
    use tokio::process::Command;

    let git_url = normalize_repo_url_for_git(repo_url);
    let output = Command::new("git")
    .args(&["ls-remote", "--heads", "--tags", &git_url])
    .output()
    .await
    .with_context(|| {
        format!(
            "Failed to spawn git process for 'git ls-remote --heads --tags {}' (normalized from '{}')", 
            git_url,
            repo_url
        )
    })?;

    if !output.status.success() {
        return Err(anyhow::anyhow!("git ls-remote failed"));
    }

    let stdout = String::from_utf8(output.stdout)?;
    let commit_hash = stdout
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().next())
        .context("Could not parse git ls-remote output")?;

    Ok(commit_hash.to_string())
}

/// Fetch up to 10 recent commits from a git repository and insert them into the database
pub async fn fetch_and_insert_recent_commits(
    pool: &PgPool,
    repo_url: &str,
    max_commits: usize,
) -> Result<Vec<String>> {
    let max_commits = std::cmp::min(max_commits, 10);
    let commit_hashes = get_recent_commit_hashes(repo_url, max_commits).await?;

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

/// Get recent commit hashes using git log
async fn get_recent_commit_hashes(repo_url: &str, max_commits: usize) -> Result<Vec<String>> {
    use tokio::process::Command;

    let git_url = normalize_repo_url_for_git(repo_url);
    let output = Command::new("git")
    .args(&["ls-remote", "--heads", "--tags", &git_url])
    .output()
    .await
    .with_context(|| {
        format!(
            "Failed to spawn git process for 'git ls-remote --heads --tags {}' (normalized from '{}')", 
            git_url,
            repo_url
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

    let stdout = String::from_utf8(output.stdout)?;
    let mut commits = Vec::new();

    // Get HEAD first if available
    if let Some(head_line) = stdout.lines().find(|line| line.ends_with("HEAD")) {
        if let Some(commit_hash) = head_line.split_whitespace().next() {
            commits.push(commit_hash.to_string());
        }
    }

    // If we don't have HEAD, get the main/master branch
    if commits.is_empty() {
        for branch_name in &["refs/heads/main", "refs/heads/master"] {
            if let Some(branch_line) = stdout.lines().find(|line| line.ends_with(branch_name)) {
                if let Some(commit_hash) = branch_line.split_whitespace().next() {
                    commits.push(commit_hash.to_string());
                    break;
                }
            }
        }
    }

    // For getting multiple commits, we need to use git log which requires cloning
    // Since you want to avoid that, we'll just return the latest commit
    // If you really need multiple commits, we'd need to do a shallow clone

    if commits.is_empty() {
        return Err(anyhow::anyhow!("No commits found in repository"));
    }

    Ok(commits)
}

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

        match fetch_and_insert_recent_commits(pool, &flake.repo_url, 10).await {
            Ok(commits) => {
                info!(
                    "✅ Successfully initialized {} commits for {}",
                    commits.len(),
                    flake.name
                );
            }
            Err(e) => {
                warn!("❌ Failed to initialize commits for {}: {}", flake.name, e);
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

        match fetch_and_insert_latest_commit(pool, &flake.repo_url).await {
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

use crate::models::commits::Commit;
use crate::models::config;
use crate::queries::commits::{flake_has_commits, flake_last_commit, insert_commit};
use anyhow::{Context, Result, bail};
use sqlx::PgPool;
use tracing::{debug, info, warn};

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
    let commits = get_commits_with_timestamps(repo_url, branch, limit, None).await?;

    let mut inserted = Vec::new();
    for (hash, timestamp) in commits {
        if let Err(e) = insert_commit(pool, &hash, repo_url, timestamp).await {
            warn!("Failed to insert commit {}: {}", hash, e);
        } else {
            inserted.push(hash);
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

        match fetch_and_insert_recent_commits(
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
) -> Result<()> {
    info!(
        "🔄 Syncing commits for {} watched flakes",
        watched_flakes.len()
    );

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
                                if !new_commits.is_empty() {
                                    info!(
                                        "✅ Found {} new commits for {}",
                                        new_commits.len(),
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
                match fetch_and_insert_recent_commits(
                    pool,
                    &flake.repo_url,
                    &flake.branch(),
                    Some(flake.initial_commit_depth),
                )
                .await
                {
                    Ok(commits) => {
                        info!(
                            "✅ Successfully initialized {} commits for {}",
                            commits.len(),
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

    Ok(())
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
async fn get_commits_with_timestamps(
    repo_url: &str,
    branch: &str,
    limit: Option<usize>,
    since_commit: Option<&str>,
) -> Result<Vec<(String, chrono::DateTime<chrono::Utc>)>> {
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

    // Build git log args
    let mut args = vec!["log", "--format=%H|%cI"];

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

    if !log_output.status.success() {
        let stderr = String::from_utf8_lossy(&log_output.stderr);
        bail!("git log failed: {}", stderr.trim());
    }

    let stdout = String::from_utf8(log_output.stdout)?;
    let commits: Result<Vec<_>> = stdout
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let parts: Vec<&str> = line.split('|').collect();
            if parts.len() != 2 {
                bail!("Invalid git log format: {}", line);
            }
            let hash = parts[0].trim().to_string();
            let timestamp = chrono::DateTime::parse_from_rfc3339(parts[1].trim())
                .context("Failed to parse timestamp")?
                .with_timezone(&chrono::Utc);
            Ok((hash, timestamp))
        })
        .collect();

    commits
}

/// Fetch and insert all new commits since a given commit hash
pub async fn fetch_and_insert_commits_since(
    pool: &PgPool,
    repo_url: &str,
    branch: &str,
    since_commit: &Commit,
) -> Result<Vec<String>> {
    let commits = get_commits_with_timestamps(
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
    for (hash, timestamp) in commits.into_iter().rev() {
        if let Err(e) = insert_commit(pool, &hash, repo_url, timestamp).await {
            warn!("Failed to insert commit {}: {}", hash, e);
        } else {
            debug!("✅ Inserted commit {} for {}", hash, repo_url);
            inserted.push(hash);
        }
    }

    info!(
        "✅ Inserted {} new commits for {}",
        inserted.len(),
        repo_url
    );
    Ok(inserted)
}

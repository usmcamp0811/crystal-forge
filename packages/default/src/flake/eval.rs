use crate::models::commits::Commit;
use crate::queries::commits::increment_commit_list_attempt_count;
use anyhow::{Context, Result};
use sqlx::PgPool;
use std::path::Path;
use tokio::process::Command;
use tokio::time::{Duration, timeout};
use tracing::{debug, error};

/// Returns the list of NixOS configurations defined in a flake at a given Git commit.
///
/// If `repo_url` is a local filesystem path, this will attempt to run
/// `git checkout <commit>` inside that path before evaluating the flake.
///
/// # Arguments
/// - `repo_url`: Git URL or filesystem path to a Nix flake
/// - `commit`: Git commit hash to evaluate the flake at
///
/// # Returns
/// A list of configuration names under `nixosConfigurations` in the flake.
///
/// # Errors
/// Returns an error if:
/// - `git checkout` fails (for local paths)
/// - `nix flake show` fails
/// - `nixosConfigurations` is missing from the flake output
pub async fn list_nixos_configurations_from_commit(
    pool: &PgPool,
    commit: &Commit,
) -> Result<Vec<String>> {
    let flake = commit.get_flake(pool).await?;
    let repo_url = &flake.repo_url;
    let commit_hash = &commit.git_commit_hash;

    debug!(
        "🔍 list_nixos_configurations_from_commit called with repo_url={repo_url} commit={commit_hash}"
    );

    let is_path = Path::new(repo_url).exists();

    let flake_uri = if is_path {
        repo_url.to_string()
    } else if repo_url.starts_with("git+") {
        format!("{}?rev={}", repo_url, commit_hash)
    } else {
        let separator = if repo_url.contains('?') { "&" } else { "?" };
        let git_suffix = if repo_url.contains('?') { "" } else { ".git" };
        format!("git+{}{git_suffix}{separator}rev={}", repo_url, commit_hash)
    };

    if is_path {
        let status = Command::new("git")
            .args(["-C", repo_url, "checkout", commit_hash])
            .status()
            .await?;

        if !status.success() {
            anyhow::bail!(
                "git checkout failed for path {} at rev {}",
                repo_url,
                commit_hash
            );
        }
    }

    let output = timeout(
        Duration::from_secs(300),
        Command::new("nix")
            .args(["flake", "show", "--json", &flake_uri])
            .output(),
    )
    .await??;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        error!("❌ nix flake show failed for {flake_uri}: {stderr}");
        match increment_commit_list_attempt_count(&pool, &commit).await {
            Ok(_) => tracing::debug!(
                "✅ Incremented attempt count for commit: {}",
                commit.git_commit_hash
            ),
            Err(inc_err) => tracing::error!("❌ Failed to increment attempt count: {inc_err}"),
        }
        anyhow::bail!("nix flake show failed: {}", stderr.trim());
    }

    let flake_json: serde_json::Value = serde_json::from_slice(&output.stdout)?;

    let nixos_configs = flake_json["nixosConfigurations"]
        .as_object()
        .context("missing nixosConfigurations")?
        .keys()
        .cloned()
        .collect::<Vec<String>>();

    debug!("✅ nixosConfigurations: {:?}", nixos_configs);
    Ok(nixos_configs)
}

/// Build a Nix flake URI with a ref parameter from a repository URL and branch.
///
/// Handles various URL formats correctly:
/// - Preserves existing `git+` prefix
/// - Doesn't double-add `.git` suffix
/// - Properly appends ref parameter to existing query strings
/// - Handles both HTTP(S) URLs and shorthand formats
fn build_flake_uri_with_ref(repo_url: &str, branch: &str) -> String {
    // Check if repo_url already has git+ prefix
    let has_git_prefix = repo_url.starts_with("git+");
    let base_url = if has_git_prefix {
        &repo_url[4..] // Strip git+ to process the URL
    } else {
        repo_url
    };

    // Check if URL already ends with .git
    let needs_git_suffix = !base_url.ends_with(".git") 
        && !base_url.contains('?') 
        && (base_url.starts_with("http://") || base_url.starts_with("https://"));

    // Build the normalized URL with .git suffix if needed
    let normalized_url = if needs_git_suffix {
        format!("{}.git", base_url)
    } else {
        base_url.to_string()
    };

    // Determine separator for ref parameter
    let separator = if normalized_url.contains('?') { "&" } else { "?" };

    // Build final URI with git+ prefix and ref parameter
    format!("git+{}{separator}ref={}", normalized_url, branch)
}

/// Refresh a flake's cached git repository by forcing Nix to re-fetch from remote.
///
/// This is useful when a flake repository has been force-pushed or its git history
/// has been rewritten, causing Nix's cached clone to have stale references.
///
/// # Arguments
/// - `repo_url`: The flake repository URL (e.g., "git+https://github.com/user/repo")
/// - `branch`: The branch to refresh (e.g., "main")
///
/// # Returns
/// Ok(()) if the refresh succeeded
///
/// # Errors
/// Returns an error if `nix flake update --refresh` fails
pub async fn refresh_flake_cache(repo_url: &str, branch: &str) -> Result<()> {
    // Normalize repo_url to a proper Nix flake URI with ref parameter
    let flake_uri = build_flake_uri_with_ref(repo_url, branch);

    debug!("🔄 Refreshing flake cache for: {}", flake_uri);

    // Use `nix flake metadata --refresh` to force Nix to re-fetch from remote
    // This is safer than `nix flake update` which might modify lock files
    let output = timeout(
        Duration::from_secs(60),
        Command::new("nix")
            .args(&["flake", "metadata", "--refresh", "--json", &flake_uri])
            .output(),
    )
    .await
    .context("Timeout refreshing flake cache")?
    .context("Failed to spawn nix flake metadata command")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        error!("❌ nix flake metadata --refresh failed for {}: {}", flake_uri, stderr);
        anyhow::bail!("Failed to refresh flake cache: {}", stderr.trim());
    }

    debug!("✅ Successfully refreshed flake cache for: {}", flake_uri);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_flake_uri_with_ref() {
        // HTTPS URL without .git suffix
        assert_eq!(
            build_flake_uri_with_ref("https://github.com/user/repo", "main"),
            "git+https://github.com/user/repo.git?ref=main"
        );

        // HTTPS URL with .git suffix already present
        assert_eq!(
            build_flake_uri_with_ref("https://github.com/user/repo.git", "dev"),
            "git+https://github.com/user/repo.git?ref=dev"
        );

        // git+ prefix already present
        assert_eq!(
            build_flake_uri_with_ref("git+https://github.com/user/repo", "main"),
            "git+https://github.com/user/repo.git?ref=main"
        );

        // git+ prefix with .git suffix
        assert_eq!(
            build_flake_uri_with_ref("git+https://github.com/user/repo.git", "feature"),
            "git+https://github.com/user/repo.git?ref=feature"
        );

        // URL with existing query parameters
        assert_eq!(
            build_flake_uri_with_ref("https://example.com/repo?shallow=1", "main"),
            "git+https://example.com/repo?shallow=1&ref=main"
        );

        // git+ URL with existing query parameters
        assert_eq!(
            build_flake_uri_with_ref("git+https://example.com/repo.git?shallow=1", "dev"),
            "git+https://example.com/repo.git?shallow=1&ref=dev"
        );

        // Non-HTTP URL (like github:owner/repo shorthand) - no .git suffix
        assert_eq!(
            build_flake_uri_with_ref("github:nixos/nixpkgs", "nixos-unstable"),
            "git+github:nixos/nixpkgs?ref=nixos-unstable"
        );

        // SSH URL - no .git suffix
        assert_eq!(
            build_flake_uri_with_ref("git@github.com:user/repo", "main"),
            "git+git@github.com:user/repo?ref=main"
        );

        // SSH URL with .git suffix
        assert_eq!(
            build_flake_uri_with_ref("git@github.com:user/repo.git", "main"),
            "git+git@github.com:user/repo.git?ref=main"
        );
    }
}

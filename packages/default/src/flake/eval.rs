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
        format!(
            "git+{}{}.git?rev={}",
            repo_url,
            if repo_url.ends_with('/') { "" } else { "" },
            commit_hash
        )
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

// pub async fn evaluate_derivations(
//     systems: Vec<String>,
//     flake_path: &str,
//     commit_hash: &str,
//     insert_system_fn: Arc<
//         dyn Fn(String, String, String) -> Pin<Box<dyn Future<Output = Result<()>> + Send>>
//             + Send
//             + Sync,
//     >,
//     handle_result: Arc<
//         Mutex<dyn FnMut(String, String) -> Pin<Box<dyn Future<Output = Result<()>> + Send>> + Send>,
//     >,
// ) -> Result<()> {
//     info!(
//         "🌀 starting stream_derivations for {} systems at commit {}",
//         systems.len(),
//         commit_hash
//     );
//
//     // insert all systems into db
//     for system in &systems {
//         insert_system_name(commit_hash, flake_path, system).await?;
//     }
//
//     let path = flake_path.to_string();
//     let commit = commit_hash.to_string();
//     let insert_system_fn = insert_system_fn.clone();
//
//     let systems_with_insert = futures::future::join_all(systems.into_iter().map(|system| {
//         let insert_system_fn = insert_system_fn.clone();
//         let repo_url = path.clone();
//         let commit = commit.clone();
//         async move {
//             insert_system_fn(commit.clone(), repo_url.clone(), system.clone()).await?;
//             Ok(system)
//         }
//     }))
//     .await
//     .into_iter()
//     .collect::<Result<Vec<_>>>()?;
//
//     debug!("✅ systems_with_insert: {:?}", systems_with_insert);
//
//     stream::iter(systems_with_insert)
//         .for_each_concurrent(8, {
//             let path = path.clone();
//             let commit = commit.clone();
//             let handle_result = handle_result.clone();
//
//             move |system: String| {
//                 let path = path.clone();
//                 let commit = commit.clone();
//                 let handle_result = handle_result.clone();
//
//                 async move {
//                     debug!("🔧 fetching derivation for system: {}", system);
//                     match get_system_derivation(&system, &path, &commit).await {
//                         Ok(hash) => {
//                             debug!("📦 got derivation: {} => {}", system, hash);
//                             let hash_ref = &hash;
//                             if let Err(e) =
//                                 handle_result.lock().await(system.clone(), hash.clone()).await
//                             {
//                                 error!("❌ handler failed for {}: {:?}", system, e);
//                             }
//                             if let Err(e) =
//                                 update_derivation_hash(&commit, &path, &system, hash_ref).await
//                             {
//                                 error!(
//                                     "❌ failed to insert derivation hash for {}: {:?}",
//                                     system, e
//                                 );
//                             }
//                         }
//                         Err(e) => {
//                             error!("❌ failed to get derivation for {}: {:?}", system, e);
//                         }
//                     }
//                 }
//             }
//         })
//         .await;
//
//     info!("🎯 finished stream_derivations for commit {}", commit_hash);
//     Ok(())
// }

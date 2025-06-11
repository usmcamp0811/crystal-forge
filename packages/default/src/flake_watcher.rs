use anyhow::Result;
use futures::future::join_all;
use serde_json::Value;
use tokio::process::Command;

/// Parses a Nix flake and extracts the defined NixOS configuration names.
///
/// # Arguments
///
/// * `repo_url` - A string containing the path or URL to a Nix flake.
///
/// # Returns
///
/// * A `Result<()>` that logs the list of NixOS system names defined in the flake.
///
/// # Errors
///
/// Returns an error if the `nix flake show` command fails or if the output
/// cannot be parsed as valid JSON.
pub fn get_nixos_configurations(repo_url: String) -> Result<()> {
    let flake_show = Command::new("nix")
        .args(["flake", "show", "--json", &repo_url])
        .output()?;
    let flake_json: serde_json::Value = serde_json::from_slice(&flake_show.stdout)?;
    let nixos_configs = flake_json["nixosConfigurations"]
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect::<Vec<_>>();

    println!("nixosConfigurations: {:?}", nixos_configs);
    Ok(())
}

/// Returns the derivation output path (hash) for a specific NixOS system from a given flake.
///
/// # Arguments
///
/// * `system` - The name of the NixOS configuration (e.g., "x86_64-linux").
/// * `flake_url` - A flake reference (local path or remote URL, e.g., `git+https://...`).
///
/// # Returns
///
/// * A `Result<String>` containing the derivation output path (e.g., `/nix/store/<hash>-toplevel`).
///
/// # Example
///
/// ```rust
/// let hash = get_system_derivation("x86_64-linux", "git+https://gitlab.com/example/dotfiles").await?;
/// ```
pub async fn get_system_derivation(system: &str, flake_url: &str) -> Result<String> {
    let target = format!("{flake_url}#nixosConfigurations.{system}.config.system.build.toplevel");

    let output = Command::new("nix")
        .args(["build", &target, "--dry-run", "--json"])
        .output()
        .await?;

    let parsed: Value = serde_json::from_slice(&output.stdout)?;
    let hash = parsed
        .get(0)
        .and_then(|v| v["outputs"]["out"].as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing derivation output in nix build output"))?
        .to_string();

    Ok(hash)
}

/// Fetches derivation output paths for all provided NixOS system configurations in parallel.
///
/// # Arguments
///
/// * `systems` - A list of NixOS system names (e.g., `"x86_64-linux"`, `"aarch64-linux"`).
/// * `flake_url` - A Nix flake reference (local path or remote, e.g., `git+https://...`).
///
/// # Returns
///
/// * A `Result` containing a list of tuples, where each tuple includes the system name and
///   its corresponding derivation output path.
///
/// # Errors
///
/// Returns an error if any derivation fails to resolve or if the Nix command fails.
///
/// # Example
///
/// ```rust
/// let systems = vec!["x86_64-linux".into(), "aarch64-linux".into()];
/// let results = get_all_derivations(systems, "git+https://gitlab.com/example/flake").await?;
/// ```
pub async fn get_all_derivations(
    systems: Vec<String>,
    flake_url: &str,
) -> Result<Vec<(String, String)>> {
    let tasks = systems.into_iter().map(|system| {
        let path = flake_url.to_string();
        async move {
            let hash = get_system_derivation(&system, &path).await?;
            Ok((system, hash)) as Result<_>
        }
    });

    let results = join_all(tasks).await;
    results.into_iter().collect()
}

//! GC root management for builder-side derivations.

use tracing::warn;

/// Returns the path for a GC root for the given derivation.
/// Creates the GC root directory if needed.
pub async fn get_gc_root_path(derivation_id: i32) -> String {
    let gc_root_dir = std::env::var("CRYSTAL_FORGE_GC_ROOT_DIR").unwrap_or_else(|_| {
        if std::path::Path::new("/var/cache/crystal-forge").exists()
            || std::fs::create_dir_all("/var/cache/crystal-forge/gc-roots").is_ok()
        {
            "/var/cache/crystal-forge/gc-roots".to_string()
        } else {
            format!("{}/crystal-forge/gc-roots", std::env::temp_dir().display())
        }
    });

    if let Err(e) = tokio::fs::create_dir_all(&gc_root_dir).await {
        warn!("Failed to create GC root directory {}: {}", gc_root_dir, e);
        let temp_gc_dir = format!("{}/crystal-forge/gc-roots", std::env::temp_dir().display());
        tokio::fs::create_dir_all(&temp_gc_dir)
            .await
            .expect("failed to create GC root directory in temp");
        return format!("{}/derivation-{}", temp_gc_dir, derivation_id);
    }

    format!("{}/derivation-{}", gc_root_dir, derivation_id)
}

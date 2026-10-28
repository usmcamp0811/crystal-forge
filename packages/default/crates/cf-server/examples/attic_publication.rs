//! Exercises the normal server-local publication function in a disposable VM.
//! Private config arrives on stdin. Only static outcome evidence is printed.

use cf_config::cache_credentials::CachePublicationSource;
use cf_config::config::{BuildConfig, CacheConfig, CacheType};
use crystal_forge::derivations::Derivation;
use serde::Deserialize;
use std::io::Read;

#[derive(Deserialize)]
struct Input {
    config: Option<CacheConfig>,
    #[serde(default)]
    database_destination: Option<DatabaseDestination>,
    derivation: Derivation,
    path: String,
    #[serde(default)]
    database: bool,
    #[serde(default)]
    cancel: bool,
}

/// Selects a migrated CF row inside the disposable fixture, never an API URL.
#[derive(Deserialize)]
struct DatabaseDestination {
    database_url: String,
    id: i32,
    encryption_key_file: String,
}

// The Python fixture supplies the service's key in the child environment. Rust
// does not mutate process-global environment while Tokio threads are running.
async fn selected_config(input: &DatabaseDestination) -> anyhow::Result<CacheConfig> {
    let key = std::fs::read_to_string(&input.encryption_key_file)?;
    anyhow::ensure!(std::env::var("CRYSTAL_FORGE_SECRET_KEY")? == key);
    let pool = sqlx::PgPool::connect(&input.database_url).await?;
    let destination =
        crystal_forge::queries::cache_destinations::get_cache_destination(&pool, input.id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("fixture destination missing"))?;
    anyhow::ensure!(destination.enabled && destination.cache_type == "Attic");
    // Match the normal local worker's Attic mapping. Credentials come only from
    // the public query helper's authenticated decryption of the selected row.
    Ok(CacheConfig {
        cache_type: CacheType::Attic,
        push_to: destination.push_to,
        push_after_build: true,
        signing_key: destination.signing_key_path,
        compression: destination.compression,
        parallel_uploads: destination.parallel_uploads.unwrap_or(1) as u32,
        attic_token: destination.attic_token,
        attic_cache_name: destination.attic_cache_name,
        attic_public_key: destination.attic_public_key,
        attic_ignore_upstream_cache_filter: destination
            .attic_ignore_upstream_cache_filter
            .unwrap_or(true),
        attic_jobs: destination.attic_jobs.unwrap_or(5) as u32,
        max_retries: destination.max_retries.unwrap_or(3) as u32,
        retry_delay_seconds: destination.retry_delay_seconds.unwrap_or(5) as u64,
        push_timeout_seconds: destination.push_timeout_seconds.unwrap_or(3600) as u64,
        force_repush: destination.force_repush.unwrap_or(false),
        require_sigs: destination.require_sigs.unwrap_or(true),
        ..CacheConfig::default()
    })
}

#[tokio::main]
async fn main() {
    let mut bytes = Vec::new();
    if std::io::stdin().read_to_end(&mut bytes).is_err() {
        std::process::exit(2);
    }
    let Ok(input) = serde_json::from_slice::<Input>(&bytes) else {
        std::process::exit(2);
    };
    let (config, database) = if let Some(selected) = &input.database_destination {
        if input.config.is_some() {
            std::process::exit(2);
        }
        let Ok(config) = selected_config(selected).await else {
            std::process::exit(2);
        };
        (config, true)
    } else {
        let Some(config) = input.config else {
            std::process::exit(2);
        };
        (config, input.database)
    };
    let task = tokio::spawn(async move {
        input
            .derivation
            .push_to_cache_with_source_and_retry(
                &input.path,
                &config,
                &BuildConfig::default(),
                if database {
                    CachePublicationSource::Database
                } else {
                    CachePublicationSource::Static
                },
            )
            .await
    });
    if input.cancel {
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        task.abort();
        let _ = task.await;
        for _ in 0..500 {
            let pending = std::fs::read_dir("/tmp").unwrap().any(|entry| {
                entry
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .starts_with("cf-cache-")
            });
            if !pending {
                println!("cancel_cleanup_complete=true");
                std::process::exit(1);
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        std::process::exit(3);
    }
    let result = task.await.unwrap();
    println!("publication_success={}", result.is_ok());
    if result.is_err() {
        std::process::exit(1);
    }
}

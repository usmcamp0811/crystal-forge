//! Exercises the normal builder mapping and publication function in a disposable VM.
//! Input is private JSON on stdin; only the publication outcome reaches stdout.

use cf_builder::{build::Derivation, cache::builder_cache_to_config};
use cf_config::config::{BuildConfig, CacheConfig};
use cf_protocol::builder::BuilderCachePushConfig;
use serde::Deserialize;
use std::io::Read;

#[derive(Deserialize)]
struct Input {
    push: BuilderCachePushConfig,
    derivation: Derivation,
    path: String,
    #[serde(default)]
    cancel: bool,
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
    let config = builder_cache_to_config(&input.push, &CacheConfig::default());
    let task = tokio::spawn(async move {
        input
            .derivation
            .push_to_cache_with_source_and_retry(
                &input.path,
                &config,
                &BuildConfig::default(),
                if input.push.cache_destination_id.is_some() {
                    cf_config::cache_credentials::CachePublicationSource::Database
                } else {
                    cf_config::cache_credentials::CachePublicationSource::Static
                },
            )
            .await
    });
    if input.cancel {
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        task.abort();
        let _ = task.await;
        // Let the detached owner finish kill/reap before the runtime exits.
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
    // Never print the error chain: the old path includes raw Attic bodies.
    if result.is_err() {
        std::process::exit(1);
    }
}

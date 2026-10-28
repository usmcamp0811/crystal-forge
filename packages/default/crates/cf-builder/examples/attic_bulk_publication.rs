//! Verifies selected Attic chunk arguments and native uploads in a disposable VM.
//! Private wire configuration arrives on stdin. Child output is discarded.

use cf_builder::cache::builder_cache_to_config;
use cf_config::CacheConfig;
use cf_config::cache_credentials::PreparedAtticPush;
use cf_protocol::builder::BuilderCachePushConfig;
use serde::Deserialize;
use std::io::Read;
use std::process::Stdio;

#[derive(Deserialize)]
struct Input {
    push: BuilderCachePushConfig,
    paths: Vec<String>,
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
    if input.push.cache_destination_id.is_none() {
        std::process::exit(2);
    }
    let cache = builder_cache_to_config(&input.push, &CacheConfig::default());
    let Ok(prepared) = PreparedAtticPush::for_paths(&cache, &input.paths) else {
        println!("bulk_preparation_success=false");
        std::process::exit(1);
    };
    let Ok(urls) = cf_config::resolve_attic_urls(
        cache.push_to.as_deref().unwrap_or_default(),
        cache.attic_cache_name.as_deref().unwrap_or_default(),
    ) else {
        drop(prepared);
        std::process::exit(2);
    };
    let mut expected = vec![
        "push".to_owned(),
        format!("cf-selected:{}", urls.cache_name),
    ];
    expected.extend(input.paths);
    if cache.attic_ignore_upstream_cache_filter {
        expected.push("--ignore-upstream-cache-filter".into());
    }
    expected.extend(["--jobs".into(), cache.attic_jobs.to_string()]);
    if prepared.args != expected
        || prepared.args.iter().any(|arg| arg == "--no-closure")
        || prepared.args.iter().any(|arg| {
            cache
                .attic_token
                .as_deref()
                .is_some_and(|token| arg.contains(token))
        })
    {
        drop(prepared);
        std::process::exit(2);
    }
    println!("bulk_args_verified=true token_in_argv=false");
    let mut command = tokio::process::Command::new("attic");
    command
        .args(&prepared.args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    prepared.apply_to_command(&mut command);
    let Ok(mut child) = command.spawn() else {
        drop(prepared);
        std::process::exit(1);
    };
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(cache.push_timeout_seconds),
        child.wait(),
    )
    .await;
    let success = matches!(result, Ok(Ok(status)) if status.success());
    if child.id().is_some() && child.kill().await.is_err() {
        // Keep credentials available if the verification child cannot be reaped.
        std::mem::forget(prepared);
        std::process::exit(3);
    }
    drop(prepared);
    println!("bulk_publication_success={success}");
    if !success {
        std::process::exit(1);
    }
}

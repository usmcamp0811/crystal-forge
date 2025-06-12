use serde_json::Value;

// Make a Queue that gets populated by webhook triggers
// this will keep us from being bogged down by trying to eval too many systems at once
// we could also in the future distribute this queue to multiple workers

// queue = []
// queue.append(webhook trigger)
// all_systems = get_nixos_configurations(repo_url: String)
// stream_derivations(all_systems) -> Postgres function to save derivation hash to crystal_forge.system_build table

fn extract_repo_info(payload: &serde_json::Value) -> Option<(String, String)> {
    let repo_url = payload
        .pointer("/repository/clone_url") // GitHub
        .or(payload.pointer("/project/web_url")) // GitLab
        .and_then(|v| v.as_str())?;

    let commit_hash = payload
        .pointer("/after") // GitHub
        .or_else(|| payload.pointer("/checkout_sha")) // GitLab
        .and_then(|v| v.as_str())?;

    Some((repo_url.to_string(), commit_hash.to_string()))
}

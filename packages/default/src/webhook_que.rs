// Make a Queue that gets populated by webhook triggers
// this will keep us from being bogged down by trying to eval too many systems at once
// we could also in the future distribute this queue to multiple workers

// queue = []
// queue.append(webhook trigger)
// all_systems = get_nixos_configurations(repo_url: String)
// stream_derivations(all_systems) -> Postgres function to save derivation hash to crystal_forge.system_build table

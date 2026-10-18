#!/usr/bin/env bash
# Run the real evaluator fixture without touching the daemon store or user cache.
set -euo pipefail
root=$(git rev-parse --show-toplevel)
workspace=$(mktemp -d)
# Nix makes ingested fixture directories read-only. Restore permissions only
# inside this command's disposable workspace so cleanup preserves test status.
trap 'chmod -R u+w "$workspace"; rm -rf "$workspace"' EXIT
export HOME="$workspace/home" XDG_CACHE_HOME="$workspace/cache"
export NIX_REMOTE="$workspace/store"
export NIX_CONFIG='experimental-features = nix-command flakes
substituters =
extra-substituters =
accept-flake-config = false
download-attempts = 1
stalled-download-timeout = 120
http-connections = 1'
mkdir -p "$HOME" "$XDG_CACHE_HOME" "$workspace/results"
cd "$workspace"
python3 "$root/modules/nixos/crystal-forge/tests/discovery.py" \
  "$root/packages/default/crates/cf-server/src/models/primary_evaluation.nix" \
  "$workspace/results"

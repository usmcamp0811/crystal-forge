{ pkgs, ... }:

# The loopback endpoint and nested store exist only in this build sandbox.
# Use the same Nix library as the authoritative bulk evaluator.
pkgs.runCommand "crystal-forge-nixos-discovery-reliability" {
  nativeBuildInputs = [ pkgs.python3 pkgs.nix-eval-jobs.nix pkgs.nix-eval-jobs ];
} ''
  export HOME="$TMPDIR/home"
  export XDG_CACHE_HOME="$TMPDIR/cache"
  export NIX_REMOTE="$TMPDIR/store"
  export NIX_CONFIG='experimental-features = nix-command flakes
  substituters =
  extra-substituters =
  accept-flake-config = false
  download-attempts = 1
  stalled-download-timeout = 120
  http-connections = 1'
  mkdir -p "$HOME" "$XDG_CACHE_HOME" "$out"
  python3 ${./test.py} \
    ${../../packages/default/crates/cf-server/src/models/primary_evaluation.nix} \
    "$out"
''

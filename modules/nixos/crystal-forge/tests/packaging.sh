#!/usr/bin/env bash
# Execute generated module scripts and installed wrappers without another
# package/check derivation. --module-only avoids building Rust components.
set -euo pipefail
module_only=false
case "${1-}" in
  '') ;;
  --module-only) module_only=true ;;
  *) exit 2 ;;
esac
workspace=$(mktemp -d)
trap 'rm -rf "$workspace"' EXIT
export TMPDIR="$workspace"
expression="let
  f = builtins.getFlake (\"git+file://\" + toString ./.);
  p = import f.inputs.nixpkgs {
    system = builtins.currentSystem;
    overlays = [ f.overlays.default ];
  };
in import ./modules/nixos/crystal-forge/tests/packaging.nix {
  inputs = f.inputs // { self = f; };
  pkgs = p;
  lib = p.lib // { crystal-forge = f.lib; };
  runtime = true;
  moduleOnly = $module_only;
}"
nix eval --impure --raw --expr "$expression" > "$workspace/probe.sh"
# Realize referenced generated configs/wrappers directly. A string evaluation
# alone does not build its context; no synthetic verification output is needed.
nix eval --impure --json --expr "builtins.attrNames (builtins.getContext ($expression))" \
  | jq -r '.[] | select(endswith(".drv"))' \
  | xargs -r nix-store --realise --builders '' --max-jobs 1 --cores 2 >/dev/null
# The rendered script writes wrapper probes using relative paths. Confine
# those scratch files to this invocation, never the repository worktree.
(cd "$workspace"; bash "$workspace/probe.sh")

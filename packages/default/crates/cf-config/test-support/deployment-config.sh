#!/usr/bin/env bash
# Run from the repository root through `nix develop .#sqlx -c bash <this-file>`.
# Prepare the shell/Cargo sources and evaluate deployment-config.nix with only
# https://cache.nixos.org enabled before running this offline regression.
# No generated script or service is executed. Nix only realizes TOML/script data.
set -euo pipefail
test "$PWD" = "$(git rev-parse --show-toplevel)"
support=packages/default/crates/cf-config/test-support
temporary=$(mktemp -d)
trap 'rm -rf "$temporary"' EXIT
nix eval --impure --raw --offline --no-update-lock-file \
  --option substituters '' --option substitute false --option builders '' \
  --option fallback false --option accept-flake-config false --max-jobs 1 \
  --expr 'import ./packages/default/crates/cf-config/test-support/deployment-config.nix { root = toString ./.; }' \
  > "$temporary/generated.toml"
# Keep the normal unit-test fixture tied to real module output.
cmp "$support/deployment-config.toml" "$temporary/generated.toml"
CF_TEST_GENERATED_DEPLOYMENT_CONFIG="$temporary/generated.toml" \
  CARGO_NET_OFFLINE=true SQLX_OFFLINE=true \
  cargo test --offline --locked --manifest-path packages/default/Cargo.toml \
    -p cf-config generated_nixos_deployment_config_roundtrips -- --nocapture

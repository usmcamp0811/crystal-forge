#!/usr/bin/env bash
# Runs the existing broad state-machine groups and unique database regressions
# in one sqlx shell and compilation directory. Only the launcher owns the cluster.
set -euo pipefail
source "$PROJECT_ROOT/packages/devScripts/tests/database-environment.sh"
"$BASH" "$PROJECT_ROOT/packages/devScripts/tests/database-regressions.sh"

# These broad groups remain the primary owner of their six overlaps with the
# extracted regressions. Do not narrow the groups to the historical overlap.
for testFilter in \
  queries::commits::tests \
  queries::derivations::tests \
  models::evaluate_with_policies::tests::finalize_attempt_ \
  models::evaluate_with_policies::tests::finalize_system_ \
  models::evaluate_with_policies::tests::system_build_ordering_persist_then_root_then_activate \
  models::evaluate_with_policies::tests::migration_0184_
do
  cargo test --manifest-path Cargo.toml -p cf-server --lib "$testFilter" \
    -- --ignored --test-threads=1
done

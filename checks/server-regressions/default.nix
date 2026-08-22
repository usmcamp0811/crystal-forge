{ pkgs, ... }:

# Critical PostgreSQL-backed Rust regressions that are intentionally outside
# the normal cf-server package build.
#
# Why this check exists:
# `packages/default/default.nix` builds/tests cf-server with `--lib --bins`.
# Cargo integration targets under `crates/cf-server/tests/` are therefore not
# compiled or executed by `nix build .#server`. The general NixOS integration
# check runs the Python cf_test suite, not these Rust targets.
#
# Keep this list focused on data-integrity contracts. Do not turn this into an
# indiscriminate `--all-targets --ignored` gate: the repository also contains
# intentionally manual, slow, and environment-specific ignored tests.
pkgs.rustPlatform.buildRustPackage {
  pname = "crystal-forge-server-regressions";
  version = "0.1.0";

  src = ../../packages/default;
  cargoLock.lockFile = ../../packages/default/Cargo.lock;

  # Build the server library once; the custom check phase below compiles and
  # runs only the selected integration targets.
  cargoBuildFlags = [ "--package" "cf-server" "--lib" ];

  nativeBuildInputs = with pkgs; [
    pkg-config
    postgresql
    postgresqlTestHook
    sqlx-cli
  ];
  buildInputs = with pkgs; [
    openssl
    libressl
  ];

  doCheck = true;
  SQLX_OFFLINE = "true";

  # sqlx::test creates disposable databases. The hook's test role therefore
  # needs CREATEDB; SUPERUSER also lets migration tests exercise trigger and
  # immutability behavior exactly as production migrations define it.
  postgresqlTestUserOptions = "LOGIN SUPERUSER CREATEDB";
  postgresqlExtraSettings = "shared_preload_libraries = 'pg_stat_statements'";
  postgresqlEnableTCP = 1;
  PGUSER = "cf_regression";
  PGDATABASE = "cf_regression";

  checkPhase = ''
    runHook preCheck

    export DATABASE_URL="postgresql://$PGUSER@127.0.0.1/$PGDATABASE"
    export CRYSTAL_FORGE_TEST_DATABASE_URL="$DATABASE_URL"

    # Shared-database tests (resolver/deletion and selected live lib tests)
    # expect the schema to exist before they start. sqlx::test targets create
    # their own isolated databases from this same migration source.
    cargo sqlx migrate run --source crates/cf-server/migrations

    echo "=== Critical cf-server integration targets ==="
    cargo test --offline --package cf-server \
      --test assignment_semantics \
      --test evidence_for_ato \
      --test framework_version_id_lifecycle \
      --test policy_counts_defect \
      --test time_window_policy_test \
      -- --test-threads=1

    echo "=== Resolver exact-version/enforcement regressions ==="
    cargo test --offline --package cf-server \
      --test resolver_enforcement \
      -- --ignored --test-threads=1

    echo "=== Immutable deletion lifecycle regressions ==="
    cargo test --offline --package cf-server \
      --test deletion_lifecycle \
      -- --ignored --test-threads=1

    echo "=== Exact trusted STIG mapping-pair regressions ==="
    cargo test --offline --package cf-server --lib \
      stig_import_succeeds_with_unrelated_trusted_mapping \
      -- --ignored --test-threads=1
    cargo test --offline --package cf-server --lib \
      stig_import_fails_closed_when_exact_mapping_pair_is_not_trusted \
      -- --ignored --test-threads=1

    echo "=== Bundle requirement lifecycle regression ==="
    cargo test --offline --package cf-server --lib \
      requirement_baseline_lifecycle_is_ordered_atomic_and_digest_independent \
      -- --ignored --test-threads=1

    echo "=== Bundle aggregate query-count regression ==="
    cargo test --offline --package cf-server --lib \
      bundle_summary_aggregate_query_count_is_bounded_across_versions \
      -- --ignored --test-threads=1

    runHook postCheck
  '';

  # This is a check derivation, not a distributable package.
  installPhase = ''
    mkdir -p "$out"
    printf '%s\n' "critical cf-server PostgreSQL regressions passed" > "$out/result"
  '';
}

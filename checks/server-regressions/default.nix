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
    nix
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

    echo "=== Populated pre-0233 migration rehearsal ==="
    createdb cf_upgrade
    upgradeUrl="postgresql://$PGUSER@127.0.0.1/cf_upgrade"
    pre0233="$TMPDIR/migrations-through-0232"
    mkdir -p "$pre0233"
    expectedTask433Migrations=0
    for migration in crates/cf-server/migrations/*.sql; do
      base="''${migration##*/}"
      version="''${base%%_*}"
      if (( 10#$version <= 232 )); then
        cp "$migration" "$pre0233/"
      fi
      if (( 10#$version >= 233 )); then
        expectedTask433Migrations=$((expectedTask433Migrations + 1))
      fi
    done
    DATABASE_URL="$upgradeUrl" cargo sqlx migrate run --source "$pre0233"
    psql "$upgradeUrl" -v ON_ERROR_STOP=1 <<'SQL'
      INSERT INTO users(id,username,first_name,last_name,email)
      VALUES('43360000-0000-0000-0000-000000000001','poam-upgrade','POAM','Upgrade','poam-upgrade@example.invalid');
      INSERT INTO environments(id,name)
      VALUES('43360000-0000-0000-0000-000000000002','POAM upgrade environment');
      INSERT INTO systems(id,hostname,environment_id,public_key,derivation)
      VALUES('43360000-0000-0000-0000-000000000003','poam-upgrade-system',
        '43360000-0000-0000-0000-000000000002','poam-upgrade-key','poam-upgrade-key');
      INSERT INTO deployment_policies(id,name,policy_type,config,enabled)
      VALUES('43360000-0000-0000-0000-000000000004','POAM upgrade policy',
        'custom_check','{"expression":"true"}'::jsonb,true);
      INSERT INTO system_policies(system_id,policy_id)
      VALUES('43360000-0000-0000-0000-000000000003',
        '43360000-0000-0000-0000-000000000004');
      INSERT INTO flakes(id,name,repo_url)
      VALUES(433600,'POAM upgrade flake','https://example.invalid/poam-upgrade.git');
      INSERT INTO commits(id,flake_id,git_commit_hash,commit_timestamp,evaluation_status)
      VALUES(433600,433600,repeat('a',40),NOW()-INTERVAL '2 days','complete');
      UPDATE systems SET flake_id=433600,
        desired_target='/nix/store/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-poam-upgrade'
      WHERE id='43360000-0000-0000-0000-000000000003';
      INSERT INTO derivations(
        id,commit_id,derivation_type,derivation_name,derivation_path,status_id,
        expected_store_path,store_path,cf_agent_enabled,policy_requirements_met,
        completed_at,policy_results)
      VALUES(433600,433600,'nixos','poam-upgrade-system',
        '/nix/store/bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb-poam-upgrade.drv',
        (SELECT id FROM derivation_statuses ORDER BY id LIMIT 1),
        '/nix/store/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-poam-upgrade',
        '/nix/store/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-poam-upgrade',TRUE,TRUE,
        NOW()-INTERVAL '1 day','{}'::jsonb);
      INSERT INTO cve_scans(
        id,derivation_id,status,scanner_name,critical_count,high_count,completed_at,created_at)
      VALUES('43360000-0000-0000-0000-000000000005',433600,'completed','upgrade-rehearsal',2,3,
        NOW()-INTERVAL '12 hours',NOW()-INTERVAL '1 day');
      INSERT INTO system_states(hostname,store_path,change_reason,timestamp)
      VALUES('poam-upgrade-system','/nix/store/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-poam-upgrade',
        'cf_deployment',NOW()-INTERVAL '10 hours');
      INSERT INTO compliance_bundles(id,name,framework,version,owner)
      VALUES('43360000-0000-0000-0000-000000000006','POAM upgrade bundle','custom','1.0.0','Upgrade');
      INSERT INTO compliance_bundle_assignments(
        id,bundle_id,bundle_version_id,scope_type,system_id,enforcement_mode,
        assignment_overlay_digest,created_by,updated_by)
      SELECT '43360000-0000-0000-0000-000000000007',bundle.id,version.id,
        'system','43360000-0000-0000-0000-000000000003','enforce','upgrade-overlay',
        '43360000-0000-0000-0000-000000000001','43360000-0000-0000-0000-000000000001'
      FROM compliance_bundles bundle
      JOIN compliance_bundle_versions version ON version.id=bundle.current_draft_version_id
      WHERE bundle.id='43360000-0000-0000-0000-000000000006';
      INSERT INTO compliance_bundle_assignment_versions(
        id,assignment_id,version_number,bundle_version_id,enforcement_mode,
        assignment_overlay_digest,created_by)
      SELECT '43360000-0000-0000-0000-000000000008',assignment.id,1,
        assignment.bundle_version_id,'enforce','upgrade-overlay',
        '43360000-0000-0000-0000-000000000001'
      FROM compliance_bundle_assignments assignment
      WHERE assignment.id='43360000-0000-0000-0000-000000000007';
      UPDATE compliance_bundle_assignments
      SET current_version_id='43360000-0000-0000-0000-000000000008'
      WHERE id='43360000-0000-0000-0000-000000000007';
      INSERT INTO attention_occurrences(
        id,category,subject_type,subject_id,source_occurrence_key,
        opened_at,last_observed_at,resolved_at,metadata)
      VALUES('43360000-0000-0000-0000-000000000009','builds','build_job',
        '43360000-0000-0000-0000-000000000010','poam-upgrade-attention',
        NOW()-INTERVAL '40 days',NOW()-INTERVAL '39 days',NOW()-INTERVAL '38 days','{}');
      INSERT INTO user_notification_preferences(user_id,delivery_channel,build_failures,initialized_at)
      VALUES('43360000-0000-0000-0000-000000000001','in_app',TRUE,NOW()-INTERVAL '60 days')
      ON CONFLICT(user_id) DO UPDATE SET delivery_channel=EXCLUDED.delivery_channel;
      INSERT INTO user_notifications(
        id,user_id,category,source_occurrence_id,source_type,source_id,title,summary,route,
        in_app_visible,read_at)
      VALUES('43360000-0000-0000-0000-000000000011',
        '43360000-0000-0000-0000-000000000001','build_failures',
        '43360000-0000-0000-0000-000000000009','builds',
        '43360000-0000-0000-0000-000000000010','Upgrade build','Preserved notification',
        '/builds',TRUE,NOW()-INTERVAL '30 days');
SQL
    DATABASE_URL="$upgradeUrl" cargo sqlx migrate run --source crates/cf-server/migrations
    appliedTask433Migrations="$(psql "$upgradeUrl" -Atc \
       'SELECT COUNT(*) FROM _sqlx_migrations WHERE version >= 233')"
    if [[ "$appliedTask433Migrations" != "$expectedTask433Migrations" ]]; then
      echo "Expected $expectedTask433Migrations task migrations through current; applied $appliedTask433Migrations" >&2
      exit 1
    fi
    psql "$upgradeUrl" -v ON_ERROR_STOP=1 <<'SQL'
      SELECT check_name,passed FROM (VALUES
        ('system',EXISTS(SELECT 1 FROM systems WHERE id='43360000-0000-0000-0000-000000000003')),
        ('policy',EXISTS(SELECT 1 FROM deployment_policies WHERE id='43360000-0000-0000-0000-000000000004')),
        ('cve_scan',EXISTS(SELECT 1 FROM cve_scans WHERE id='43360000-0000-0000-0000-000000000005' AND composite_phase_order IS NOT NULL)),
        ('desired_target',EXISTS(SELECT 1 FROM composite_legacy_desired_targets WHERE system_id='43360000-0000-0000-0000-000000000003' AND target_store_path='/nix/store/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-poam-upgrade')),
        ('assignment_version',EXISTS(SELECT 1 FROM compliance_bundle_assignment_versions WHERE id='43360000-0000-0000-0000-000000000008' AND assignment_id='43360000-0000-0000-0000-000000000007')),
        ('system_state',EXISTS(SELECT 1 FROM system_states WHERE hostname='poam-upgrade-system' AND store_path='/nix/store/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-poam-upgrade')),
        ('attention',EXISTS(SELECT 1 FROM attention_occurrences WHERE id='43360000-0000-0000-0000-000000000009')),
        ('notification',EXISTS(SELECT 1 FROM user_notifications WHERE id='43360000-0000-0000-0000-000000000011' AND read_at IS NOT NULL AND materialization_order IS NOT NULL)),
        ('bootstrap_incomplete',EXISTS(SELECT 1 FROM user_notification_source_bootstrap_state WHERE singleton AND completed_at IS NULL)),
        ('deployment_failure_index',to_regclass('system_events_deployment_failure_bootstrap_idx') IS NOT NULL)
      ) checks(check_name,passed);
      DO $$
      BEGIN
        IF NOT EXISTS (
          SELECT 1 FROM systems WHERE id='43360000-0000-0000-0000-000000000003'
        ) OR NOT EXISTS (
          SELECT 1 FROM deployment_policies WHERE id='43360000-0000-0000-0000-000000000004'
        ) OR NOT EXISTS (
          SELECT 1 FROM cve_scans
          WHERE id='43360000-0000-0000-0000-000000000005'
            AND composite_phase_order IS NOT NULL
        ) OR NOT EXISTS (
          SELECT 1 FROM composite_legacy_desired_targets
          WHERE system_id='43360000-0000-0000-0000-000000000003'
            AND target_store_path='/nix/store/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-poam-upgrade'
        ) OR NOT EXISTS (
          SELECT 1 FROM compliance_bundle_assignment_versions
          WHERE id='43360000-0000-0000-0000-000000000008'
            AND assignment_id='43360000-0000-0000-0000-000000000007'
        ) OR NOT EXISTS (
          SELECT 1 FROM system_states
          WHERE hostname='poam-upgrade-system'
            AND store_path='/nix/store/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-poam-upgrade'
        ) OR NOT EXISTS (
          SELECT 1 FROM attention_occurrences
          WHERE id='43360000-0000-0000-0000-000000000009'
        ) OR NOT EXISTS (
          SELECT 1 FROM user_notifications
          WHERE id='43360000-0000-0000-0000-000000000011'
            AND read_at IS NOT NULL AND materialization_order IS NOT NULL
        ) OR NOT EXISTS (
          SELECT 1 FROM user_notification_source_bootstrap_state
          WHERE singleton AND completed_at IS NULL
        ) OR to_regclass('system_events_deployment_failure_bootstrap_idx') IS NULL
        THEN
          RAISE EXCEPTION 'pre-0233 populated rows were not preserved';
        END IF;
      END
      $$;
SQL

    echo "=== Critical cf-server integration targets ==="
    cargo test --offline --package cf-server \
      --test assignment_semantics \
      --test composite_policy \
      --test evidence_for_ato \
      --test framework_version_id_lifecycle \
      --test policy_counts_defect \
      --test policy_editor_phase2 \
      --test poam_workflows \
      --test task433_csrf \
      --test time_window_policy_test \
      -- --test-threads=1

    echo "=== Selected POA&M authorization, setup, notification, and overdue regressions ==="
    cargo test --offline --package cf-server --lib \
      handlers::api::poam::tests::http_requires_session_csrf_and_mutator_role \
      -- --ignored --test-threads=1
    cargo test --offline --package cf-server --lib \
      handlers::api::setup_wizard::tests::setup_progress_counts_production_policy_bundle_and_poam_rows \
      -- --ignored --test-threads=1
    cargo test --offline --package cf-server --lib \
      queries::user_notifications::tests:: \
      -- --ignored --test-threads=1
    cargo test --offline --package cf-server --lib \
      tasks::user_notification_email::tests:: \
      -- --ignored --test-threads=1
    cargo test --offline --package cf-server --lib \
      tasks::attention_reconciliation::tests::poam_overdue_reconciliation_deduplicates_resolves_and_opens_new_episode \
      -- --ignored --test-threads=1

    echo "=== Composite AC3 pure validation and interchange matrix ==="
    cargo test --offline --package cf-server --lib \
      ac3_validation_matrix_accepts_and_rejects_each_exposed_kind_discriminately \
      -- --test-threads=1
    cargo test --offline --package cf-server --lib \
      composite_json_toml_and_cf_native_interchange_preserve_all_supported_rules \
      -- --test-threads=1

    echo "=== Composite AC3 authoritative Nix executor matrix ==="
    cargo test --offline --package cf-server --lib \
      ac3_actual_nix_executor_matrix_distinguishes_pass_fail_error_and_evidence \
      -- --ignored --test-threads=1

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

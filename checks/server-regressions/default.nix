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

    echo "=== Populated pre-0248 immutable-artifact upgrade rehearsal ==="
    createdb cf_snapshot_upgrade
    snapshotUpgradeUrl="postgresql://$PGUSER@127.0.0.1/cf_snapshot_upgrade"
    pre0248="$TMPDIR/migrations-through-0247"
    mkdir -p "$pre0248"
    for migration in crates/cf-server/migrations/*.sql; do
      base="''${migration##*/}"
      version="''${base%%_*}"
      if (( 10#$version <= 247 )); then
        cp "$migration" "$pre0248/"
      fi
    done
    DATABASE_URL="$snapshotUpgradeUrl" cargo sqlx migrate run --source "$pre0248"
    psql "$snapshotUpgradeUrl" -v ON_ERROR_STOP=1 <<'SQL'
      INSERT INTO flakes(id,name,repo_url,branch)
      VALUES(440248,'Snapshot upgrade','https://example.invalid/snapshot-upgrade.git','main');
      INSERT INTO commits(id,flake_id,git_commit_hash,commit_timestamp,evaluation_status)
      VALUES(440248,440248,repeat('a',40),NOW()-INTERVAL '2 days','failed');
      INSERT INTO systems(id,hostname,public_key,derivation,flake_id,system_configuration_name)
      VALUES('44024800-0000-0000-0000-000000000001','snapshot-upgrade-host',
        'snapshot-upgrade-key','snapshot-upgrade-key',440248,'snapshot-upgrade-host');
      INSERT INTO derivations(
        id,commit_id,derivation_type,derivation_name,derivation_path,status_id,
        expected_store_path,store_path,cf_agent_enabled,policy_requirements_met,
        completed_at,policy_results)
      VALUES(440248,440248,'nixos','snapshot-upgrade-host',
        '/nix/store/bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb-snapshot-upgrade.drv',
        (SELECT id FROM derivation_statuses ORDER BY id LIMIT 1),
        '/nix/store/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-snapshot-upgrade',
        '/nix/store/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-snapshot-upgrade',TRUE,TRUE,
        NOW()-INTERVAL '1 day','{}'::jsonb);
      INSERT INTO evaluation_option_contents(digest,payload,search_text)
      VALUES(decode(repeat('01',32),'hex'),
        '{"declared_type":"string","value":{"kind":"scalar","value":"safe"},"definitions":[],"overridden":false}'::jsonb,
        'safe');
      INSERT INTO evaluation_snapshots(
        id,commit_id,configuration_name,lifecycle,option_count,module_count,
        content_bytes,completed_at)
      VALUES('44024800-0000-0000-0000-000000000002',440248,
        'snapshot-upgrade-host','available',1,0,100,NOW()-INTERVAL '1 day');
      INSERT INTO evaluation_snapshot_options(
        snapshot_id,option_path,content_digest,is_overridden)
      VALUES('44024800-0000-0000-0000-000000000002','services.safe',
        decode(repeat('01',32),'hex'),FALSE);
      INSERT INTO evaluation_generation_snapshots(
        system_id,generation,snapshot_id,derivation_id,commit_id,source_store_path)
      VALUES('44024800-0000-0000-0000-000000000001',7,
        '44024800-0000-0000-0000-000000000002',440248,440248,
        '/nix/store/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-snapshot-upgrade');
      UPDATE evaluation_snapshots
      SET lifecycle='failed',error='later retry failed',option_count=0,module_count=0,
          content_bytes=0,completed_at=NOW()
      WHERE id='44024800-0000-0000-0000-000000000002';

      INSERT INTO commits(id,flake_id,git_commit_hash,commit_timestamp,evaluation_status)
      VALUES(440249,440248,repeat('b',40),NOW()-INTERVAL '1 day','complete');
      INSERT INTO systems(id,hostname,public_key,derivation,flake_id,system_configuration_name)
      VALUES('44024800-0000-0000-0000-000000000003','ambiguous-upgrade-host',
        'ambiguous-upgrade-key','ambiguous-upgrade-key',440248,'ambiguous-upgrade-host');
      INSERT INTO derivations(
        id,commit_id,derivation_type,derivation_name,derivation_path,status_id,
        expected_store_path,store_path,cf_agent_enabled,policy_requirements_met,
        completed_at,policy_results)
      VALUES(440249,440249,'nixos','ambiguous-upgrade-host',
        '/nix/store/dddddddddddddddddddddddddddddddd-ambiguous-upgrade.drv',
        (SELECT id FROM derivation_statuses ORDER BY id LIMIT 1),
        '/nix/store/cccccccccccccccccccccccccccccccc-ambiguous-upgrade',
        '/nix/store/cccccccccccccccccccccccccccccccc-ambiguous-upgrade',TRUE,TRUE,
        NOW()-INTERVAL '12 hours','{}'::jsonb);
      INSERT INTO evaluation_option_contents(digest,payload,search_text)
      VALUES(decode(repeat('02',32),'hex'),
        '{"declared_type":"string","value":{"kind":"scalar","value":"replacement"},"definitions":[],"overridden":false}'::jsonb,
        'replacement');
      INSERT INTO evaluation_snapshots(
        id,commit_id,configuration_name,lifecycle,option_count,module_count,
        content_bytes,completed_at)
      VALUES('44024800-0000-0000-0000-000000000004',440249,
        'ambiguous-upgrade-host','available',1,0,100,NOW()-INTERVAL '12 hours');
      INSERT INTO evaluation_snapshot_options(
        snapshot_id,option_path,content_digest,is_overridden)
      VALUES('44024800-0000-0000-0000-000000000004','services.safe',
        decode(repeat('01',32),'hex'),FALSE);
      INSERT INTO evaluation_generation_snapshots(
        system_id,generation,snapshot_id,derivation_id,commit_id,source_store_path)
      VALUES('44024800-0000-0000-0000-000000000003',8,
        '44024800-0000-0000-0000-000000000004',440249,440249,
        '/nix/store/cccccccccccccccccccccccccccccccc-ambiguous-upgrade');
      INSERT INTO pending_system_deployments(
        id,system_id,target_store_path,status,source,requested_commit_id)
      VALUES('44024800-0000-0000-0000-000000000005',
        '44024800-0000-0000-0000-000000000003',
        '/nix/store/cccccccccccccccccccccccccccccccc-ambiguous-upgrade',
        'succeeded','manual',440249);
      UPDATE evaluation_snapshot_options
      SET content_digest=decode(repeat('02',32),'hex')
      WHERE snapshot_id='44024800-0000-0000-0000-000000000004'
        AND option_path='services.safe';
      UPDATE evaluation_snapshots
      SET completed_at=NOW()
      WHERE id='44024800-0000-0000-0000-000000000004';
      UPDATE derivations
      SET store_path=NULL,expected_store_path=NULL
      WHERE id=440249;

      INSERT INTO evaluation_option_contents(digest,payload,search_text)
      VALUES
        (decode(repeat('03',32),'hex'),
         '{"declared_type":"boolean","value":{"kind":"scalar","value":true},"definitions":[{"source_path":"modules/count.nix","winning":true}],"overridden":false}'::jsonb,
         'module count mismatch'),
        (decode(repeat('04',32),'hex'),
         '{"declared_type":"boolean","value":{"kind":"scalar","value":true},"definitions":[],"overridden":true}'::jsonb,
         'override mismatch');
      INSERT INTO evaluation_snapshots(
        id,commit_id,configuration_name,lifecycle,option_count,module_count,
        content_bytes,completed_at)
      VALUES
        ('44024800-0000-0000-0000-000000000007',440249,
         'module-count-mismatch','available',1,0,100,NOW()),
        ('44024800-0000-0000-0000-000000000008',440249,
         'override-mismatch','available',1,0,100,NOW());
      INSERT INTO evaluation_snapshot_options(
        snapshot_id,option_path,content_digest,is_overridden)
      VALUES
        ('44024800-0000-0000-0000-000000000007','services.count',
         decode(repeat('03',32),'hex'),FALSE),
        ('44024800-0000-0000-0000-000000000008','services.override',
         decode(repeat('04',32),'hex'),FALSE);
SQL
    DATABASE_URL="$snapshotUpgradeUrl" cargo sqlx migrate run --source crates/cf-server/migrations
    psql "$snapshotUpgradeUrl" -v ON_ERROR_STOP=1 <<'SQL'
      DO $$
      DECLARE
        recovered uuid;
        certification_bypass_accepted boolean := false;
      BEGIN
        SELECT snapshot_id INTO recovered
        FROM evaluation_generation_snapshots
        WHERE system_id='44024800-0000-0000-0000-000000000001' AND generation=7;
        IF recovered='44024800-0000-0000-0000-000000000002' OR NOT EXISTS (
          SELECT 1 FROM evaluation_snapshots snapshot
          WHERE snapshot.id=recovered AND snapshot.lifecycle='available'
            AND snapshot.option_count=1 AND snapshot.integrity_version=1
        ) OR NOT EXISTS (
          SELECT 1 FROM evaluation_snapshot_options item
          WHERE item.snapshot_id=recovered AND item.option_path='services.safe'
        ) OR NOT EXISTS (
          SELECT 1 FROM evaluation_snapshot_selections selection
          JOIN evaluation_snapshots snapshot ON snapshot.id=selection.current_snapshot_id
          WHERE selection.commit_id=440248
            AND selection.configuration_name='snapshot-upgrade-host'
            AND snapshot.lifecycle='failed'
        ) OR NOT EXISTS (
          SELECT 1 FROM evaluation_generation_snapshots retained
          WHERE retained.system_id='44024800-0000-0000-0000-000000000001'
            AND retained.generation=7 AND NOT retained.lineage_verified
        ) THEN
          RAISE EXCEPTION '0248 did not preserve failed current and recover retained success';
        END IF;
        IF NOT EXISTS (
          SELECT 1
          FROM evaluation_generation_snapshots retained
          JOIN evaluation_snapshots snapshot ON snapshot.id=retained.snapshot_id
          JOIN derivations derivation ON derivation.id=retained.derivation_id
          WHERE retained.system_id='44024800-0000-0000-0000-000000000003'
            AND retained.generation=8
            AND snapshot.lifecycle='available'
            AND snapshot.integrity_version=1
            AND NOT retained.lineage_verified
            AND derivation.store_path IS NULL
            AND derivation.expected_store_path IS NULL
        ) OR NOT EXISTS (
          SELECT 1 FROM pending_system_deployments
          WHERE id='44024800-0000-0000-0000-000000000005'
            AND evaluation_snapshot_id IS NULL
            AND NOT evaluation_snapshot_binding_expected
        ) THEN
          RAISE EXCEPTION '0248 overclaimed ambiguous legacy deployment or retained lineage';
        END IF;
        IF to_regclass('evaluation_snapshot_selections_snapshot_idx') IS NULL
          OR to_regclass('evaluation_generation_snapshots_snapshot_idx') IS NULL
          OR to_regclass('evaluation_generation_snapshots_derivation_idx') IS NULL
          OR to_regclass('pending_system_deployments_evaluation_snapshot_idx') IS NULL THEN
          RAISE EXCEPTION '0248 reverse foreign-key indexes are incomplete';
        END IF;
        BEGIN
          INSERT INTO evaluation_snapshots(
            id,commit_id,configuration_name,lifecycle,integrity_version)
          VALUES('44024800-0000-0000-0000-000000000006',440249,
            'direct-certification-bypass','unavailable',1);
          certification_bypass_accepted := true;
        EXCEPTION WHEN OTHERS THEN
          NULL;
        END;
        IF certification_bypass_accepted THEN
          RAISE EXCEPTION '0248 allowed integrity certification during artifact insertion';
        END IF;
        IF EXISTS (
          SELECT 1 FROM evaluation_snapshots
          WHERE id IN (
            '44024800-0000-0000-0000-000000000007',
            '44024800-0000-0000-0000-000000000008'
          ) AND integrity_version <> 0
        ) THEN
          RAISE EXCEPTION '0248 certified mismatched module or override metadata';
        END IF;
        IF evaluation_safe_option_value_valid(
             '{"kind":"scalar","value":[]}'::jsonb
           ) OR evaluation_safe_option_value_valid(
             '{"kind":"scalar","value":{}}'::jsonb
           ) THEN
          RAISE EXCEPTION '0248 accepted a collection tagged as scalar';
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
      --test task433_assignment_visibility \
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
      queries::compliance::tests::policy_requirement_identity_hydration_uses_exact_versions \
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

    echo "=== Evaluation comparison baseline failure isolation ==="
    cargo test --offline --package cf-server --lib \
      comparison_baseline_failures_are_isolated_for_commits_and_generations \
      -- --ignored --test-threads=1
    cargo test --offline --package cf-server --lib \
      bounded_comparison_page_handles_multi_thousand_option_snapshots \
      -- --ignored --test-threads=1

    echo "=== Exported-module declaration pagination ==="
    cargo test --offline --package cf-server --lib \
      exported_module_declaration_pagination_is_stable_bounded_and_read_only \
      -- --ignored --test-threads=1

    echo "=== Selected evaluation summary and provenance visibility ==="
    cargo test --offline --package cf-server --lib \
      selected_evaluation_summary_is_authoritative_and_visibility_filtered \
      -- --ignored --test-threads=1
    cargo test --offline --package cf-server --lib \
      evaluation_module_sources_page_is_complete_stable_bounded_and_read_only \
      -- --ignored --test-threads=1
    cargo test --offline --package cf-server --lib \
      evaluation_module_source_count_and_continuation_follow_bounded_replacements \
      -- --ignored --test-threads=1
    cargo test --offline --package cf-server --lib \
      ac24_metrics_and_filtered_reconciliation_are_authoritative \
      -- --ignored --test-threads=1
    cargo test --offline --package cf-server --lib \
      materialized_host_delta_scales_across_large_multi_configuration_corpus \
      -- --ignored --test-threads=1
    cargo test --offline --package cf-server --lib \
      finalization_populates_host_metrics_for_complete_configuration_corpus \
      -- --ignored --test-threads=1
    cargo test --offline --package cf-server --lib \
      evaluation_start_waits_for_snapshot_writer_before_commit_lock \
      -- --ignored --test-threads=1
    cargo test --offline --package cf-server --lib \
      canonical_evaluation_queue_transition_preserves_lineage_and_finalization \
      -- --ignored --test-threads=1

    echo "=== Immutable evaluation artifact and exact-page contracts ==="
    cargo test --offline --package cf-server --lib \
      immutable_artifact_selection_retention_gc_and_rollback_are_isolated \
      -- --ignored --test-threads=1
    cargo test --offline --package cf-server --lib \
      final_audit_lineage_lifecycle_and_source_reset_fail_closed \
      -- --ignored --test-threads=1
    cargo test --offline --package cf-server --lib \
      source_reset_and_history_rewrite_preserve_durable_commit_identities \
      -- --ignored --test-threads=1
    cargo test --offline --package cf-server --lib \
      deployment_creation_and_snapshot_finalization_serialize_exact_binding_and_retention \
      -- --ignored --test-threads=1
    cargo test --offline --package cf-server --lib \
      successful_system_persistence_waits_for_snapshot_writer_lock \
      -- --ignored --test-threads=1
    cargo test --offline --package cf-server --lib \
      cross_commit_shared_path_keeps_distinct_idempotent_rows \
      -- --ignored --test-threads=1
    cargo test --offline --package cf-server --lib \
      options_page_rejects_every_malformed_variant_outside_requested_page \
      -- --ignored --test-threads=1
    cargo test --offline --package cf-server --lib \
      terminal_deployment_artifact_bindings_release_after_ingestion_window \
      -- --ignored --test-threads=1
    cargo test --offline --package cf-server --lib \
      delayed_activation_after_two_hour_expiry_retains_and_correlates_lineage \
      -- --ignored --test-threads=1
    cargo test --offline --package cf-server --lib \
      secrets_are_absent_while_safe_values_remain_searchable_in_storage_and_api \
      -- --ignored --test-threads=1
    cargo test --offline --package cf-server --lib \
      changed_query_is_symmetric_bounded_and_side_effect_free \
      -- --ignored --test-threads=1

    echo "=== TASK-440 manual and auto-latest deployment contracts ==="
    cargo test --offline --package cf-server --lib \
      manual_conversion_persists_across_failure_and_retry_reuses_deployment \
      -- --ignored --test-threads=1
    cargo test --offline --package cf-server --lib \
      failed_manual_conversion_queues_no_deployment \
      -- --ignored --test-threads=1
    cargo test --offline --package cf-server --lib \
      concurrent_explicit_request_conflicts_before_policy_conversion \
      -- --ignored --test-threads=1

    runHook postCheck
  '';

  # This is a check derivation, not a distributable package.
  installPhase = ''
    mkdir -p "$out"
    printf '%s\n' "critical cf-server PostgreSQL regressions passed" > "$out/result"
  '';
}

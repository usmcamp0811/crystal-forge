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

    # Keep the timing EXIT trap local so PostgreSQL hook cleanup retains its
    # ownership. Boundaries report real work, never periodic synthetic output.
    (
    checkStarted=$SECONDS
    phaseName=""
    phaseStarted=$SECONDS
    finishPhase() {
      local status="$1"
      if [[ -n "$phaseName" ]]; then
        printf 'END phase=%s exit=%s elapsed=%ss at=%s\n' \
          "$phaseName" "$status" "$((SECONDS - phaseStarted))" "$(date -u +%FT%TZ)"
      fi
    }
    beginPhase() {
      finishPhase 0
      phaseName="$1"
      phaseStarted=$SECONDS
      printf 'START phase=%s at=%s\n' "$phaseName" "$(date -u +%FT%TZ)"
    }
    finishCheck() {
      local status="$?"
      finishPhase "$status"
      printf 'END check=server-regressions exit=%s elapsed=%ss at=%s\n' \
        "$status" "$((SECONDS - checkStarted))" "$(date -u +%FT%TZ)"
      exit "$status"
    }
    trap finishCheck EXIT
    printf 'START check=server-regressions at=%s\n' "$(date -u +%FT%TZ)"

    # Only caller-supplied target/mode labels are printed, never argv or env.
    # A failed command retains its actual status even under Bash errexit.
    runRegressionCommand() {
      local testName="$1" mode="$2" started=$SECONDS status
      shift 2
      printf 'START %s mode=%s at=%s\n' "$testName" "$mode" "$(date -u +%FT%TZ)"
      if "$@"; then
        status=0
      else
        status=$?
      fi
      printf 'END %s mode=%s exit=%s elapsed=%ss at=%s\n' \
        "$testName" "$mode" "$status" "$((SECONDS - started))" "$(date -u +%FT%TZ)"
      return "$status"
    }

    beginPhase migrations-and-upgrade-rehearsals
    export DATABASE_URL="postgresql://$PGUSER@127.0.0.1/$PGDATABASE"
    export CRYSTAL_FORGE_TEST_DATABASE_URL="$DATABASE_URL"
    # Public sandbox-only fixture key. Never inherit deployment credentials.
    export CRYSTAL_FORGE_CACHE_ENCRYPTION_KEY="server-regressions-public-fixture-key"

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
    beginPhase critical-integration-targets
    # Preserve all eleven targets and Cargo's failure propagation. Separate
    # compilation/linking from execution to identify a silent boundary.
    for testTarget in \
      assignment_semantics \
      compliance_assignment_zombie_repair \
      composite_policy \
      evidence_for_ato \
      framework_version_id_lifecycle \
      policy_counts_defect \
      policy_editor_phase2 \
      poam_workflows \
      task433_assignment_visibility \
      task433_csrf \
      time_window_policy_test
    do
      runRegressionCommand "$testTarget" compile \
        cargo test --offline --package cf-server --test "$testTarget" --no-run
      runRegressionCommand "$testTarget" run \
        cargo test --offline --package cf-server --test "$testTarget" \
          -- --test-threads=1
    done

    beginPhase selected-library-and-additional-integration-regressions
    runRegressionCommand cf-server-lib compile \
      cargo test --offline --package cf-server --lib --no-run
    echo "=== Selected POA&M authorization, setup, notification, and overdue regressions ==="
    cargo test --offline --package cf-server --lib \
      handlers::api::poam::tests::http_requires_session_csrf_and_mutator_role \
      -- --ignored --test-threads=1
    cargo test --offline --package cf-server --lib \
      handlers::api::setup_wizard::tests::setup_progress_counts_production_policy_bundle_and_poam_rows \
      -- --ignored --test-threads=1
    cargo test --offline --package cf-server --lib \
      handlers::api::compliance::tests::publish_bundle_with_seed_shaped_cve_policy_commits_and_exports \
      -- --ignored --test-threads=1
    cargo test --offline --package cf-server --lib \
      handlers::api::compliance::tests::publish_bundle_with_malformed_cve_config_rolls_back \
      -- --ignored --test-threads=1
    cargo test --offline --package cf-server --lib \
      handlers::api::compliance::tests::assignment_create_rejects_incomplete_active_lineage \
      -- --ignored --test-threads=1
    cargo test --offline --package cf-server --lib \
      handlers::api::compliance::tests::assignment_create_failure_points_roll_back_all_rows \
      -- --ignored --test-threads=1
    cargo test --offline --package cf-server --lib \
      handlers::api::compliance::tests::assignment_list_contract_and_deactivation_safety \
      -- --ignored --test-threads=1
    cargo test --offline --package cf-server --lib \
      handlers::api::compliance::tests::bundle_publication_rolls_back_when_custom_check_xccdf_is_invalid \
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

    echo "=== System CVE inventory candidate query and route regressions ==="
    cargo test --offline --package cf-server --lib \
      inventory_candidates_treat_unbuilt_derivation_as_not_current \
      -- --ignored --test-threads=1
    cargo test --offline --package cf-server --lib \
      cve_inventory_sources_route_hides_systems_and_serializes_unbuilt_targets \
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

    # TASK-435. Agent key rotation is a security boundary: a denied caller must
    # never mutate systems.public_key, and a successful rotation must persist
    # exactly one new key and audit it as system_key_rotated.
    echo "=== System agent key rotation authorization/persistence regressions ==="
    cargo test --offline --package cf-server --lib \
      system_key_rotation_ \
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
    cargo test --offline --package cf-server --lib \
      snapshot_capture_failure_persists_unavailable_with_durable_diagnostic \
      -- --ignored --test-threads=1
    cargo test --offline --package cf-server --lib \
      finalization_replaces_missing_snapshot_artifacts_with_unavailable_lifecycle \
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

    echo "=== TASK-440 immutable build retry contracts ==="
    for testName in \
      manual_requeue_creates_once_then_reuses_active_attempt \
      concurrent_manual_requeue_requests_share_one_active_attempt \
      manual_requeue_enforces_operator_environment_scope \
      visible_non_terminal_requeue_is_a_lifecycle_conflict \
      migration_0262_upgrades_populated_global_derivation_uniqueness \
      obsolete_contract_requeue_requires_exact_commit_reevaluation \
      obsolete_source_reuses_authoritative_active_child \
      stale_obsolete_marker_does_not_replace_later_terminal_attempt \
      manual_and_authoritative_obsolete_recovery_create_one_active_child \
      manual_and_automatic_retry_serialize_to_one_active_child \
      automatic_retry_creates_one_delayed_linked_child_and_keeps_source_terminal \
      manually_requeued_attempt_claims_through_production_path \
      authoritative_re_evaluation_replaces_obsolete_job_during_activation \
      visibility_scope_handles_ambiguous_systems_cache_pushes_and_eval_attempts
    do
      cargo test --offline --package cf-server --lib "$testName" \
        -- --ignored --test-threads=1
    done

    echo "=== Exact build-attempt authorization and production-scale query plan ==="
    cargo test --offline --package cf-server --lib \
      exact_build_lookup_plan_stays_primary_key_bounded_at_production_scale \
      -- --ignored --test-threads=1

    echo "=== TASK-470 cache scope, signed capability, selection, and dispatch contracts ==="
    beginPhase task470-exact-cache-regressions
    # --exact alone succeeds when a test is renamed or removed. Require both
    # the named success line and exactly one executed test. pipefail preserves
    # Cargo failures through tee; ignored tests are enabled only by name.
    runExactCacheRegression() {
      local testName="$1" mode=nonignored started=$SECONDS status
      shift
      if [[ "''${1:-}" == --ignored ]]; then
        mode=ignored
      fi
      local testLog="$TMPDIR/task470-exact-test.log"
      printf 'START %s mode=%s at=%s\n' "$testName" "$mode" "$(date -u +%FT%TZ)"
      if (set -o pipefail; cargo test --offline --package cf-server --lib "$testName" \
        -- --exact --test-threads=1 --color=never "$@" 2>&1 | tee "$testLog"); then
        status=0
      else
        status=$?
      fi
      if (( status == 0 )) && { ! grep -Fxq "test $testName ... ok" "$testLog" || \
        ! grep -Fq "test result: ok. 1 passed; 0 failed; 0 ignored;" "$testLog"; }; then
        echo "Expected exactly one successful test: $testName" >&2
        status=1
      fi
      printf 'END %s mode=%s exit=%s elapsed=%ss at=%s\n' \
        "$testName" "$mode" "$status" "$((SECONDS - started))" "$(date -u +%FT%TZ)"
      return "$status"
    }

    # TASK-487 replaces a vacuous list assertion with fixture membership in a
    # sqlx-created database. The exact guard rejects missing or renamed tests.
    runExactCacheRegression \
      handlers::api::deployment_policies::tests::test_list_deployment_policies_includes_fixture \
      --ignored

    # Exercise both refactored pagination loops and their authorization/cap
    # contracts against sandbox-owned databases. Require each of the seven
    # selected integration tests to execute; an empty target cannot pass.
    runRegressionCommand register_export_selection compile \
      cargo test --offline --package cf-server --test register_export_selection --no-run
    for testName in \
      pages_whole_filtered_scope_and_rechecks_reader \
      acceptance_only_does_not_apply_unrelated_poam_filters \
      policy_export_context_uses_actual_waiver_finding \
      exceeds_combined_cap_without_partial_selection \
      plan_context_retains_all_linked_systems_in_one_snapshot \
      retired_link_in_same_environment_exports_but_moved_out_link_does_not \
      scheduled_environment_keeps_both_exact_packages
    do
      testLog="$TMPDIR/task487-register-export-exact-test.log"
      runRegressionCommand "$testName" ignored \
        bash -o pipefail -c 'cargo test --offline --package cf-server \
          --test register_export_selection "$1" \
          -- --ignored --exact --test-threads=1 --color=never 2>&1 | tee "$2"' \
          _ "$testName" "$testLog"
      if ! grep -Fxq "test $testName ... ok" "$testLog" || \
        ! grep -Fq "test result: ok. 1 passed; 0 failed; 0 ignored;" "$testLog"; then
        echo "Expected exactly one successful test: $testName" >&2
        exit 1
      fi
    done

    # TASK-488 captures the real resolved plan and reads exact-attempt outcomes
    # in sqlx-created sandbox databases. Keep the missing/renamed-test guard.
    for testName in \
      queries::eval_policy_plans::tests::capture_is_atomic_first_write_immutable_and_retains_partial_rows \
      queries::eval_policy_plans::tests::capture_rejects_wrong_attempt_owner_and_retry_cannot_read_old_plan \
      server::policy_capture_tests::real_loader_captures_overlap_empty_contexts_and_immutable_history \
      server::policy_capture_tests::real_virtual_baseline_capture_preserves_bundle_without_assignment_uuids
    do
      runExactCacheRegression "$testName" --ignored
    done

    # MR334 review regressions use the declared module paths, including child
    # test modules. Keep each isolated SQLx case behind the exact-success guard.
    for testName in \
      queries::eval_policy_plans::scoped_read_tests::mixed_flake_projects_all_metadata_and_shared_configuration \
      queries::eval_policy_plans::scoped_read_tests::foreign_missing_and_removed_membership_are_nondisclosing \
      queries::eval_policy_plans::scoped_read_tests::deleted_moved_and_reconfigured_hosts_fail_closed \
      queries::eval_policy_plans::scoped_read_tests::authorization_and_evidence_share_repeatable_read_snapshot \
      queries::eval_policy_plans::scoped_read_tests::legacy_configuration_names_cannot_grant_historical_host_access \
      queries::eval_policy_plans::scoped_read_tests::pinned_reads_preserve_scope_and_revoke_historical_access \
      queries::commits::policy_counts::tests::native_ten_passes_then_queued_retry_has_zero_current_counts \
      queries::commits::policy_counts::tests::current_partial_native_and_mock_counts_exclude_stale_and_pending \
      queries::commits::policy_counts::tests::markerless_legacy_counts_are_unavailable_not_passing \
      handlers::api::systems::evaluation_lifecycle::tests::lifecycle_mutation_scopes_authority_audits_and_cleans_up \
      handlers::api::systems::evaluation_lifecycle::tests::lifecycle_handler_enforces_auth_csrf_and_nondisclosing_scope \
      handlers::api::systems::evaluation_lifecycle::tests::lifecycle_audit_failure_rolls_back_classification \
      server::policy_capture_tests::capture_display_metadata_and_classification_share_repeatable_read_snapshot \
      queries::eval_policy_plans::pinned_read_tests::pin_belongs_to_commit_and_never_falls_back_to_new_retry \
      queries::eval_policy_plans::pinned_read_tests::historical_mock_values_survive_retry_while_stale_native_is_unknown \
      queries::eval_policy_plans::pinned_read_tests::pinned_native_failure_is_preserved_until_its_marker_is_overwritten
    do
      runExactCacheRegression "$testName" --ignored
    done

    # TASK-488 discovery, summary, and evidence visibility use isolated SQLx
    # databases. Require every qualified case to pass the exact-success guard.
    for testName in \
      queries::evaluation_inventory::tests::inventory_normalizes_exact_names_and_rejects_replacement \
      queries::evaluation_inventory::tests::inventory_empty_and_selection_subset_are_distinct_from_missing \
      queries::evaluation_inventory::tests::inventory_schema_enforces_owner_immutability_and_parent_cascade \
      queries::evaluation_inventory::tests::inventory_rejects_cancelled_superseded_and_nonactive_writers \
      queries::evaluation_inventory::tests::inventory_retry_never_inherits_or_accepts_older_writes \
      queries::evaluation_inventory::tests::inventory_competing_first_writes_preserve_one_complete_record \
      queries::commits::inventory_summary_tests::exact_inventory_replaces_cached_41_and_excludes_unselected_outcomes \
      queries::commits::inventory_summary_tests::inventory_partial_unknown_ambiguous_and_queued_retry_remain_pending \
      queries::commits::inventory_summary_tests::scoped_summaries_hide_shared_policy_facts_search_errors_and_revoked_membership \
      handlers::api::commits::visibility::tests::mixed_environment_flake_denies_unstructured_evidence \
      handlers::api::commits::visibility::tests::revocation_rechecks_each_snapshot_and_keeps_reads_atomic \
      handlers::api::commits::visibility::tests::unmanaged_discovery_name_denies_even_with_all_memberships \
      handlers::api::commits::visibility::tests::missing_inventory_and_queued_retry_never_inherit_authority \
      handlers::api::commits::visibility::tests::all_visible_configs_allow_logs_and_mutable_latest_graph \
      handlers::api::commits::visibility::tests::shared_configuration_hidden_host_denies_even_when_inactive \
      handlers::api::commits::visibility::tests::missing_identity_or_registration_never_grants_vacuous_access \
      handlers::api::commits::visibility::tests::mutable_graph_preserves_counts_but_denies_unmanaged_retained_rows \
      handlers::api::commits::visibility::tests::stream_revalidation_observes_membership_roles_and_session_revocation \
      queries::eval_policy_plans::inventory_projection_tests::inventory_discovered_41_preserves_exact_names_and_12_captured_hosts \
      queries::eval_policy_plans::inventory_projection_tests::inventory_absent_and_unselected_registered_rows_keep_unknown_evidence \
      queries::eval_policy_plans::inventory_projection_tests::inventory_shared_hosts_count_one_matching_global_failure_without_fake_policies \
      queries::eval_policy_plans::inventory_projection_tests::inventory_history_ignores_cache_mutation_and_queued_retry_has_no_inventory \
      queries::eval_policy_plans::inventory_projection_tests::inventory_scoped_reads_hide_unmanaged_hidden_columns_and_global_failure_details \
      queries::eval_policy_plans::inventory_projection_tests::inventory_missing_discovery_is_unavailable_despite_passing_native_and_cache
    do
      runExactCacheRegression "$testName" --ignored
    done

    # TASK-488 native history uses immutable exact-attempt evidence. These SQLx
    # cases also replay the attributable 0308 import; retain the exact guard.
    for testName in \
      queries::evaluation_native_outcomes::tests::native_outcome_rejects_cancelled_superseded_and_nonactive_owners \
      queries::evaluation_native_outcomes::tests::native_outcome_requires_exact_selected_native_inventory \
      queries::evaluation_native_outcomes::tests::native_outcome_first_write_schema_immutability_and_parent_cascade \
      queries::evaluation_native_outcomes::tests::native_outcome_sidecar_has_no_artifact_build_or_observation_side_effects \
      queries::evaluation_native_outcomes::tests::native_outcome_retry_retains_historical_values_under_new_ea \
      queries::evaluation_native_outcomes::tests::native_producers_atomically_retain_success_and_confirmed_failure_across_retry
    do
      runExactCacheRegression "$testName" --ignored
    done

    # Receiver reads must enforce the same environment boundary as the matrix.
    # Exact names also ensure the scope-lock concurrency checks really execute.
    for testName in \
      handlers::api::compliance::tests::receiver_security::system_receivers_hide_out_of_scope_hosts_with_identical_404 \
      handlers::api::compliance::tests::receiver_security::bundle_systems_scope_host_details_and_every_total_before_rollup \
      handlers::api::compliance::tests::receiver_security::receiver_scope_rechecks_revocations_moves_unscoped_and_deleted_hosts \
      handlers::api::compliance::tests::receiver_security::receiver_scope_preserves_empty_revisions_and_legacy_system_rollups \
      handlers::api::compliance::tests::receiver_security::single_receiver_guard_blocks_revocation_and_move_until_materialized \
      handlers::api::compliance::tests::receiver_security::bundle_receiver_guard_bounds_candidates_and_blocks_scope_writes
    do
      runExactCacheRegression "$testName" --ignored
    done

    # TASK-484 uses sqlx-created databases in this sandbox. Run each test in a
    # separate process because the heavy-Nix semaphore is process-global.
    for testName in \
      models::evaluate_with_policies::tests::discovery_database_tests::discovery_advisory_precedes_semaphore_and_releases_before_bulk \
      models::evaluate_with_policies::tests::discovery_database_tests::discovery_quarantine_retains_real_advisory_and_capacity \
      models::evaluate_with_policies::tests::discovery_database_tests::discovery_failures_and_cancellation_terminalize_exact_attempt \
      models::evaluate_with_policies::tests::discovery_database_tests::discovery_api_cancellation_stops_active_child_and_finalizes_original_attempt \
      models::evaluate_with_policies::tests::discovery_database_tests::discovery_api_cancellation_interrupts_advisory_and_capacity_admission \
      models::evaluate_with_policies::tests::discovery_database_tests::bulk_admission_preserves_owned_leases_and_bounds_pool_wait
    do
      runExactCacheRegression "$testName" --ignored
    done

    for testName in \
      models::evaluate_with_policies::mock_persistence::tests::mock_persistence_records_attempt_outcomes_without_production_work \
      models::evaluate_with_policies::mock_persistence::tests::mock_persistence_cancelled_or_superseded_attempts_write_nothing \
      models::evaluate_with_policies::mock_persistence::tests::isolated_mock_counts_match_queue_history_and_exclude_queued_retry \
      queries::eval_policy_plans::tests::isolated_mock_projection_matches_attempt_and_rejects_ambiguous_native \
      queries::eval_policy_plans::tests::confirmed_native_finalizer_failure_is_attempt_owned_and_preserves_build \
      queries::eval_policy_plans::tests::confirmed_native_finalizer_cancellation_writes_no_failure_evidence \
      queries::commits::tests::derivation_count_isolated_current_mock_has_zero_records \
      queries::commits::tests::derivation_count_excludes_stale_native_and_queued_retry \
      queries::commits::tests::derivation_count_counts_current_native_records_not_groups \
      queries::commits::tests::derivation_count_markerless_legacy_is_unavailable \
      queries::commits::partial_resource_failure_tests::partial_resource_failure_retains_rejected_completion_with_preserved_build_history \
      queries::commits::partial_resource_failure_tests::partial_resource_failure_reconciles_committed_but_unacknowledged_completion \
      queries::commits::partial_resource_failure_tests::partial_resource_failure_ignores_never_committed_completion_candidate \
      queries::commits::partial_resource_failure_tests::partial_resource_failure_mixed_confirmed_and_resource_cohorts_terminalize \
      queries::build_jobs::tests::resource_terminal_pending_preparation_is_exact_guarded_and_activates_once \
      queries::commits::partial_resource_failure_tests::partial_resource_failure_retains_completed_evidence_snapshots_and_jobs \
      queries::commits::partial_resource_failure_tests::partial_resource_failure_rejects_unverified_retention_atomically \
      queries::commits::partial_resource_failure_tests::partial_resource_failure_respects_supersession_and_cancellation \
      handlers::api::caches::basic_read_tests::niks3_basic_get_save_test_cancel_modes_and_authority_preserve_secrets \
      queries::cache_publication_reads::tests::niks3_basic_publication_withholds_old_capabilities_and_insecure_delivery \
      handlers::api::caches::discovery_tests::niks3_stored_discovery_retains_replaces_clears_modes_without_persistence \
      handlers::api::caches::discovery_tests::niks3_stored_discovery_auth_invalid_json_type_tls_and_csrf_fail_without_mutation \
      handlers::api::caches::niks3_ca_tests::canonical_ca_get_test_cancel_and_ca_only_save_preserve_ciphertext \
      handlers::api::caches::retained_probe_tests::attic_named_cache_retained_replacement_results_and_policy_preserve_raw_state \
      handlers::api::caches::retained_probe_tests::legacy_attic_plaintext_and_historical_ciphertext_retain_on_test_and_save \
      handlers::api::caches::retained_probe_tests::legacy_attic_null_and_empty_token_refuse_before_probe_without_mutation \
      handlers::api::caches::retained_probe_tests::stored_probe_preserves_ciphertext_scope_and_timestamps \
      handlers::api::caches::retained_probe_tests::stored_probe_admin_json_missing_id_conversion_and_ssrf \
      handlers::api::caches::retained_probe_tests::add_probe_validates_without_writes_and_niks3_requires_token \
      handlers::api::caches::retained_probe_tests::legacy_basic_probe_preserves_raw_url_and_sanitized_roundtrip \
      handlers::api::caches::retained_probe_tests::legacy_query_probe_refuses_without_mutation_or_replay \
      handlers::api::caches::retained_probe_tests::legacy_s3_presigned_endpoint_redacts_and_refuses_replay \
      handlers::api::caches::retained_probe_tests::legacy_uri_type_conversion_strips_inherited_auth \
      queries::cache_destinations::atomic_scope_tests::create_scope_failure_leaves_no_cache_credentials_or_global_fallback \
      queries::cache_destinations::atomic_scope_tests::update_scope_failure_preserves_entire_config_ciphertext_and_assignments \
      queries::cache_destinations::tests::niks3_selection_assigned_first_disabled_fallback_and_stable_order \
      queries::cache_destinations::tests::niks3_assignment_writers_wait_for_publication_snapshot \
      handlers::agent::heartbeat::tests::niks3_signed_handler_preserves_pending_deployment_and_delivers_only_selected_reads \
      handlers::api::builders::tests::niks3_builder_and_agent_selection_share_assigned_first_policy \
      queries::builders::tests::niks3_canonical_environment_dispatch_rejects_ambiguity_before_credentials \
      queries::builders::tests::niks3_canonical_environment_completion_rejects_post_dispatch_ambiguity \
      queries::builders::tests::niks3_preclaim_handler_rejects_legacy_and_allows_capable_builder \
      handlers::api::builders::proxy_dispatch_tests::proxy_credential_dispatch_signed_tcp_next_job \
      queries::builders::tests::niks3_exact_candidate_claim_never_substitutes_after_queue_races \
      queries::builders::tests::niks3_dispatch_identity_and_completion_transaction_rechecks \
      queries::builders::tests::niks3_missing_push_queues_exact_id_and_requires_authoritative_output \
      queries::builders::tests::niks3_pending_publication_guards_rollback_and_later_promotion \
      queries::cache_push::niks3_tests::niks3_recorded_no_cache_dispatch_differs_from_unrecorded_enqueue \
      queries::cache_push::niks3_tests::niks3_queue_persists_provenance_pins_legacy_and_retains_deleted_identity \
      queries::cache_publication_reads::tests::niks3_publication_exact_identity_rename_and_secondary_source \
      queries::cache_publication_reads::tests::niks3_publication_private_capability_and_current_scope_retry \
      queries::cache_publication_reads::tests::niks3_publication_deleted_id_legacy_and_global_evidence \
      queries::cache_publication_reads::tests::niks3_publication_database_precedes_legacy_and_ambiguity_fails_closed \
      queries::cache_publication_reads::tests::niks3_publication_manual_pinned_auto_latest_retained_archive \
      queries::cache_publication_reads::tests::niks3_publication_unknown_historical_and_bridge_remain_retryable \
      queries::cache_publication_reads::tests::niks3_publication_rotation_race_before_claim_locks \
      queries::cache_publication_reads::tests::niks3_publication_assignment_race_before_claim_locks \
      queries::cache_publication_reads::tests::niks3_publication_deletion_race_before_claim_locks \
      queries::cache_publication_reads::tests::niks3_publication_assigned_gates_never_downgrade_to_proven_global \
      builder::cve_worker::tests::materialization_completed_provenance_identity_and_eligibility \
      handlers::api::caches::tests::niks3_api_create_rejects_malformed_keys_in_all_auth_modes \
      handlers::api::caches::tests::niks3_api_update_rejects_malformed_keys_in_all_auth_modes
    do
      runExactCacheRegression "$testName" --ignored
    done
    for testName in \
      queries::commits::partial_resource_failure_tests::partial_failure_diagnostic_uses_proven_counts_not_checkpoint_guesses \
      handlers::api::builders::tests::evaluator_fixture_reproduces_inherited_cloexec_writer \
      handlers::api::builders::tests::evaluator_fingerprint_reports_executing_nix_and_pure_contract \
      queries::commits::partial_resource_failure_tests::evaluation_failure_resource_marker_is_server_owned \
      queries::commits::partial_resource_failure_tests::partial_failure_cohort_keeps_confirmed_and_resource_systems_disjoint \
      models::retry_policy::tests::resource_failure_never_retries_whole_flake_despite_available_budget \
      models::cache_destination::tests::niks3_basic_pair_validation_redaction_modes_and_authority \
      models::cache_destination::tests::niks3_probe_planes_bootstrap_without_unselected_settings \
      handlers::api::caches::basic_read_tests::niks3_basic_http_projection_and_plane_scope_do_not_cross_credentials \
      handlers::api::caches::niks3_write_probe::tests::native_gc_schema_rejects_generic_json_and_invalid_typed_fields \
      handlers::api::caches::niks3_write_probe::tests::exact_status_media_type_and_body_are_required \
      handlers::api::caches::niks3_write_probe::tests::mtls_control_requires_a_specific_identity_alert_not_any_transport_failure \
      handlers::api::caches::niks3_write_probe::tests::nginx_omission_requires_complete_page_and_only_opens_mtls_control \
      handlers::api::caches::niks3_write_probe::tests::nginx_ingress_control_still_requires_both_native_handler_fingerprints \
      handlers::api::caches::niks3_write_probe::tests::native_token_route_pair_proves_access_and_sends_only_invalid_bodies \
      handlers::api::caches::niks3_write_probe::tests::no_post_follows_unknown_get_or_non_discriminating_authentication_control \
      handlers::api::caches::niks3_write_probe::tests::definite_native_token_denial_is_false_not_untested \
      handlers::api::caches::niks3_write_probe::tests::strict_gc_json_accepts_finished_failure_without_exposing_provider_error \
      handlers::api::caches::niks3_write_probe::tests::mtls_route_requests_send_no_bearer_and_native_subject_denial_is_false \
      handlers::api::caches::niks3_write_probe::tests::retained_and_replacement_effective_snapshots_select_only_active_write_credentials \
      handlers::api::caches::niks3_write_probe::tests::missing_or_invalid_write_token_reports_local_failure_without_dns_or_read_borrowing \
      handlers::api::caches::niks3_write_probe::tests::all_scope_invalid_bearer_preflight_precedes_metadata_dns_and_read_requests \
      handlers::api::caches::niks3_write_probe::tests::niks3_read_custom_ca_replaces_ambient_roots_and_invalid_bundles_do_not_fallback \
      handlers::api::caches::niks3_write_probe::tests::write_metadata_reachability_is_independent_of_protected_authentication \
      handlers::api::caches::niks3_write_probe::tests::all_metadata_key_mismatch_keeps_api_reachable_and_does_not_skip_authentication \
      handlers::api::caches::niks3_write_probe::tests::all_scope_missing_token_keeps_existing_local_validation_rejection \
      handlers::api::caches::niks3_write_probe::tests::generic_post_errors_and_contradictory_controls_never_verify_authorization \
      handlers::api::caches::niks3_write_probe::tests::unexpected_success_stops_without_control_retry_or_valid_request \
      handlers::api::caches::niks3_write_probe::tests::metadata_and_read_requests_do_not_receive_write_bearer \
      handlers::api::caches::niks3_write_probe::tests::chunked_bodies_are_bounded_and_a_shared_deadline_covers_stream_consumption \
      security::cache_secrets::tests::basic_password_envelope_preserves_spaces_and_literal_prefixes \
      queries::builders::tests::niks3_publication_fingerprint_includes_secrets_policy_and_canonical_sets \
      handlers::api::caches::storage_metrics::tests::native_values_preserve_basis_without_inventing_paths \
      handlers::api::caches::storage_metrics::tests::attic_inventory_preserves_exact_identity_u64_and_path_basis \
      handlers::api::caches::storage_metrics::tests::attic_inventory_rejects_missing_null_negative_overflow_and_identity_mismatch \
      handlers::api::caches::storage_metrics::tests::attic_optional_metadata_gate_requires_current_enabled_type_and_admin \
      handlers::api::caches::storage_metrics::tests::attic_mapping_is_explicit_https_and_fails_closed_for_every_invalid_entry \
      handlers::api::caches::storage_metrics::tests::attic_request_uses_only_adapter_bearer_and_exact_canonical_name \
      handlers::api::caches::storage_metrics::tests::attic_token_loader_rejects_symlinks_public_files_and_bad_tokens \
      handlers::api::caches::storage_metrics::tests::attic_protected_token_file_enforces_exact_adapter_alphabet_and_boundaries \
      handlers::api::caches::storage_metrics::tests::attic_response_is_bounded_and_status_errors_are_static \
      handlers::api::caches::storage_metrics::tests::attic_invalid_mapping_or_name_stops_before_file_or_network \
      handlers::api::caches::storage_metrics::tests::malformed_negative_missing_and_overflow_stats_never_become_zero \
      handlers::api::caches::storage_metrics::tests::metadata_gates_skip_all_unsupported_disabled_and_non_admin_probes \
      handlers::api::caches::storage_metrics::tests::missing_endpoint_denial_redirects_and_non_native_success_are_not_empty_stats \
      handlers::api::caches::storage_metrics::tests::streamed_body_limit_rejects_overflow_without_retaining_rejected_chunk \
      handlers::api::caches::storage_metrics::tests::absent_metrics_wire_shape_is_explicit_and_not_http_cached \
      handlers::api::caches::storage_metrics::tests::stats_request_keeps_configured_authority_prefix_and_no_authorization \
      handlers::api::caches::storage_metrics::tests::overall_deadline_cancels_pending_work_and_returns_nulls \
      handlers::api::caches::storage_metrics::tests::unauthenticated_request_stops_before_database_lookup \
      handlers::api::caches::discovery_tests::niks3_discovery_projects_only_write_mtls_and_public_mode \
      handlers::api::caches::discovery_tests::niks3_discovery_rejects_invalid_bundles_and_queries_before_network \
      handlers::api::caches::discovery_tests::niks3_discovery_admin_first_and_public_response_redact_tls_material \
      security::cache_secrets::tests::certificate_bundle_regressions_reject_interleaved_comments_and_noncert_blocks \
      handlers::api::caches::niks3_ca_tests::ca_normalization_keeps_roots_separate_from_strict_client_chains \
      handlers::api::caches::niks3_ca_tests::ca_diagnostics_are_typed_static_and_field_scoped \
      handlers::api::caches::niks3_ca_tests::mismatched_write_identity_fails_before_dns_and_never_proves_auth \
      handlers::api::caches::niks3_ca_tests::custom_write_roots_skip_ambient_trust_and_validate_every_root \
      handlers::api::caches::niks3_ca_tests::transport_diagnostics_use_local_causes_not_url_or_body_text \
      queries::cache_destinations::tests::retained_niks3_secrets_preserve_exact_opaque_storage \
      queries::cache_destinations::tests::effective_niks3_secret_clears_discard_original_storage \
      queries::cache_destinations::tests::ca_updates_retain_all_niks3_secret_storage_and_inactive_fields \
      handlers::api::builders::tests::attic_requisite_env_uses_shared_server_base \
      handlers::api::caches::retained_probe_tests::attic_canonical_api_and_model_read_roots_share_named_cache \
      handlers::api::caches::retained_probe_tests::attic_native_metadata_and_status_matrix_never_claims_root_or_write_success \
      handlers::api::caches::retained_probe_tests::attic_target_policy_and_credential_queries_fail_before_network \
      handlers::api::caches::retained_probe_tests::active_type_metadata_and_serialization_never_reveal_inactive_credentials \
      handlers::api::caches::retained_probe_tests::effective_update_never_borrows_inactive_credentials_on_type_conversion \
      handlers::api::caches::retained_probe_tests::bearer_auth_is_exclusive_to_active_attic \
      handlers::api::caches::retained_probe_tests::every_url_field_redacts_decoded_aws_queries_and_invalid_urls_fail_closed \
      handlers::api::caches::s3_probe::tests::sigv4_signs_exact_bucket_host_path_query_and_sensitive_session \
      handlers::agent_request::tests::niks3_capability_requires_authenticated_body_and_ignores_unsigned_headers \
      handlers::agent::heartbeat::tests::niks3_selected_cache_never_drops_private_or_unsupported_first_for_fallback \
      handlers::api::builders::tests::niks3_preclaim_capability_gate_preserves_legacy_cache_dispatch \
      handlers::api::builders::tests::cache_probe_failure_codes_and_wire_contract_are_static \
      handlers::api::builders::tests::cache_probe_exact_root_refresh_attic_and_private_capture_bounds \
      handlers::api::builders::tests::niks3_basic_probe_applies_guard_to_path_info_only \
      handlers::api::builders::tests::niks3_completion_probe_uses_independent_read_plane \
      handlers::api::builders::tests::niks3_probe_failure_never_polls_success_transition \
      handlers::api::builders::tests::niks3_probe_timeout_and_cancellation_reap_before_cleanup \
      handlers::api::builders::tests::niks3_pinned_nix_accepts_remote_metadata_and_refresh_flags \
      handlers::api::builders::niks3_input_owner_tests::niks3_input_owner_acknowledges_reap_and_cleanup_after_detach \
      handlers::api::builders::niks3_input_owner_tests::fifo_deadlines_cover_missing_readiness_and_absent_release_reader
    do
      runExactCacheRegression "$testName"
    done

    )
    runHook postCheck
  '';

  # This is a check derivation, not a distributable package.
  installPhase = ''
    mkdir -p "$out"
    printf '%s\n' "critical cf-server PostgreSQL regressions passed" > "$out/result"
  '';
}

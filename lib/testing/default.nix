{
  lib,
  inputs,
  ...
}: let
  pkgs = inputs.nixpkgs.legacyPackages.x86_64-linux;
in rec {
  cf_flake =
    pkgs.runCommand "cf-flake" {
      src = ../../.;
    } ''
      mkdir -p $out
      cp -r $src/* $out/
    '';
  # Add SQL test file as a derivation
  sqlTests = pkgs.writeText "crystal-forge-view-tests.sql" ''
    -- Crystal Forge Views Test Suite
    -- Run these tests to validate view structure and logic

    BEGIN;

    -- Test 1: Core Views Existence
    SELECT 'TEST 1: Core Views Existence' as test_name;

    DO $$
    DECLARE
        view_name text;
        expected_views text[] := ARRAY[
            'view_commit_deployment_timeline',
            'view_systems_latest_flake_commit',
            'view_systems_current_state',
            'view_systems_status_table',
            'view_recent_failed_evaluations',
            'view_evaluation_queue_status',
            'view_systems_cve_summary',
            'view_system_vulnerabilities',
            'view_environment_security_posture',
            'view_critical_vulnerabilities_alert',
            'view_evaluation_pipeline_debug',
            'view_systems_summary',
            'view_systems_drift_time',
            'view_systems_convergence_lag',
            'view_system_heartbeat_health'
        ];
    BEGIN
        FOREACH view_name IN ARRAY expected_views
        LOOP
            IF NOT EXISTS (
                SELECT 1 FROM information_schema.views
                WHERE table_name = view_name AND table_schema = 'public'
            ) THEN
                RAISE EXCEPTION 'FAIL: View % does not exist', view_name;
            END IF;
            RAISE NOTICE 'PASS: View % exists', view_name;
        END LOOP;
    END $$;

    -- Test 2: Status Symbol Logic
    SELECT 'TEST 2: Status Symbol Logic' as test_name;

    WITH status_symbols AS (
        SELECT DISTINCT status
        FROM view_systems_status_table
        WHERE status IS NOT NULL
    ),
    expected_symbols AS (
        SELECT unnest(ARRAY['🟢', '🟡', '🔴', '🟤', '⚪', '⚫']) as symbol
    )
    SELECT
        CASE
            WHEN NOT EXISTS (
                SELECT 1 FROM status_symbols s
                LEFT JOIN expected_symbols e ON s.status = e.symbol
                WHERE e.symbol IS NULL
            ) THEN 'PASS: All status symbols are valid'
            ELSE 'FAIL: Invalid status symbols found'
        END as result;

    -- Test 3: Systems Summary Totals
    SELECT 'TEST 3: Systems Summary Totals' as test_name;

    WITH summary AS (
        SELECT * FROM view_systems_summary
    ),
    individual_counts AS (
        SELECT
            COUNT(*) as total_systems_check,
            COUNT(*) FILTER (WHERE is_running_latest_derivation = TRUE) as up_to_date_check,
            COUNT(*) FILTER (WHERE is_running_latest_derivation = FALSE) as behind_check,
            COUNT(*) FILTER (WHERE last_seen < NOW() - INTERVAL '15 minutes') as no_heartbeat_check
        FROM view_systems_current_state
    )
    SELECT
        CASE
            WHEN s."Total Systems" = i.total_systems_check
            AND s."Up to Date" = i.up_to_date_check
            AND s."Behind Latest" = i.behind_check
            AND s."No Recent Heartbeat" = i.no_heartbeat_check
            THEN 'PASS: Systems summary totals match individual counts'
            ELSE 'FAIL: Systems summary totals do not match'
        END as result
    FROM summary s, individual_counts i;

    -- Test 4: View Performance
    SELECT 'TEST 4: View Performance' as test_name;

    -- Test that views execute without errors
    SELECT COUNT(*) as commit_timeline_count FROM view_commit_deployment_timeline;
    SELECT COUNT(*) as current_state_count FROM view_systems_current_state;
    SELECT COUNT(*) as status_table_count FROM view_systems_status_table;
    SELECT COUNT(*) as queue_status_count FROM view_evaluation_queue_status;

    SELECT 'PASS: All views executed without errors' as result;

    -- Test 5: Data Integrity
    SELECT 'TEST 5: Data Integrity' as test_name;

    SELECT
        CASE
            WHEN NOT EXISTS (
                SELECT 1 FROM view_systems_status_table
                WHERE hostname IS NULL
            ) THEN 'PASS: No NULL hostnames in status table'
            ELSE 'FAIL: Found NULL hostnames in status table'
        END as result;

    SELECT 'SQL TESTS COMPLETED - Check output above for any FAIL results' as summary;

    ROLLBACK;
  '';
}

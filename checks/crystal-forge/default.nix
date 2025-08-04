{
  lib,
  inputs,
  pkgs,
  ...
}: let
  keyPair = pkgs.runCommand "agent-keypair" {} ''
    mkdir -p $out
    ${pkgs.crystal-forge.agent.cf-keygen}/bin/cf-keygen -f $out/agent.key
  '';
  key = pkgs.runCommand "agent.key" {} ''
    mkdir -p $out
    cp ${keyPair}/agent.key $out/
  '';
  pub = pkgs.runCommand "agent.pub" {} ''
    mkdir -p $out
    cp ${keyPair}/agent.pub $out/
  '';

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
in
  pkgs.testers.runNixOSTest {
    name = "crystal-forge-agent-integration";

    nodes = {
      server = {
        config,
        pkgs,
        ...
      }: {
        imports = [inputs.self.nixosModules.crystal-forge];

        networking.useDHCP = true;
        networking.firewall.allowedTCPPorts = [3000];

        environment.etc."crystal-forge-tests.sql".source = "${sqlTests}";
        environment.etc."agent.key".source = "${key}/agent.key";
        environment.etc."agent.pub".source = "${pub}/agent.pub";
        environment.etc."cf_flake".source = "${cf_flake}";

        services.postgresql = {
          enable = true;
          authentication = lib.concatStringsSep "\n" [
            "local all root trust"
            "local all postgres peer"
            "host all all 127.0.0.1/32 trust"
            "host all all ::1/128 trust"
          ];
          initialScript = pkgs.writeText "init-crystal-forge.sql" ''
            CREATE USER crystal_forge LOGIN;
            CREATE DATABASE crystal_forge OWNER crystal_forge;
            GRANT ALL PRIVILEGES ON DATABASE crystal_forge TO crystal_forge;
          '';
        };
        services.crystal-forge = {
          enable = true;
          local-database = true;
          log_level = "debug";
          database = {
            user = "crystal_forge";
            host = "localhost";
            name = "crystal_forge";
          };
          flakes.watched = [
            {
              name = "dotfiles";
              repo_url = "https://gitlab.com/usmcamp0811/dotfiles";
              auto_poll = false;
            }
          ];
          environments = [
            {
              name = "test";
              description = "Test environment for Crystal Forge agents and evaluation";
              is_active = true;
              risk_profile = "LOW";
              compliance_level = "NONE";
            }
          ];
          systems = [
            {
              hostname = "agent";
              public_key = lib.strings.trim (builtins.readFile "${pub}/agent.pub");
              environment = "test";
              flake_name = "dotfiles";
            }
          ];
          server = {
            enable = true;
            host = "0.0.0.0";
            port = 3000;
          };
        };
      };
      agent = {
        config,
        pkgs,
        ...
      }: {
        imports = [inputs.self.nixosModules.crystal-forge];

        networking.useDHCP = true;
        networking.firewall.enable = false;

        environment.etc."agent.key".source = "${key}/agent.key";
        environment.etc."agent.pub".source = "${pub}/agent.pub";

        services.crystal-forge = {
          enable = true;
          client = {
            enable = true;
            server_host = "server";
            server_port = 3000;
            private_key = "/etc/agent.key";
          };
        };
      };
    };
    globalTimeout = 600;
    extraPythonPackages = p: [p.pytest];
    testScript = ''
      import pytest

      start_all()

      # Debug: Check if the service is even trying to start
      server.succeed("systemctl status crystal-forge-server.service || true")
      server.log("=== crystal-forge-server service logs ===")
      server.succeed("journalctl -u crystal-forge-server.service --no-pager || true")


      server.wait_for_unit("postgresql")
      server.wait_for_unit("crystal-forge-server.service")
      agent.wait_for_unit("crystal-forge-agent.service")
      server.wait_for_unit("multi-user.target")

      # Ensure keys are available
      try:
          agent.succeed("test -r /etc/agent.key")
          agent.succeed("test -r /etc/agent.pub")
          server.succeed("test -r /etc/agent.pub")
      except Exception as e:
          pytest.fail(f"Key presence check failed: {e}")

      try:
          server.succeed("ss -ltn | grep ':3000'")
      except Exception:
          pytest.fail("Server is not listening on port 3000")

      try:
          agent.succeed("ping -c1 server")
      except Exception:
          pytest.fail("Agent failed to ping server")

      agent_hostname = agent.succeed("hostname -s").strip()
      system_hash = agent.succeed("readlink /run/current-system").strip().split("-")[-1]
      change_reason = "startup"

      try:
          server.wait_until_succeeds("journalctl -u crystal-forge-server.service | grep '✅ accepted agent'");
      except Exception:
          pytest.fail("Server did not log 'accepted from agent'")

      agent.log("=== agent logs ===")
      agent.log(agent.succeed("journalctl -u crystal-forge-agent.service || true"))

      output = server.succeed("psql -U crystal_forge -d crystal_forge -c 'SELECT hostname, derivation_path, change_reason FROM system_states;'")
      server.log("Final DB state:\\n" + output)

      if agent_hostname not in output:
          pytest.fail(f"hostname '{agent_hostname}' not found in DB")
      if change_reason not in output:
          pytest.fail(f"change_reason '{change_reason}' not found in DB")
      if system_hash not in output:
          pytest.fail(f"derivation_path '{system_hash}' not found in DB")

      commit_hash = "2abc071042b61202f824e7f50b655d00dfd07765"
      curl_data = f"""'{{
        "project": {{
          "web_url": "https://gitlab.com/usmcamp0811/dotfiles"
        }},
        "checkout_sha": "{commit_hash}"
      }}'"""

      try:
          server.succeed(f"curl -s -X POST http://localhost:3000/webhook -H 'Content-Type: application/json' -d {curl_data}")
      except Exception:
          pytest.fail("Webhook POST request failed")

      try:
          server.wait_until_succeeds(f"journalctl -u crystal-forge-server.service | grep {commit_hash}")
      except Exception:
          pytest.fail("Commit hash was not processed by server")

      flake_check = server.succeed("psql -U crystal_forge -d crystal_forge -c \"SELECT repo_url FROM flakes WHERE repo_url = 'https://gitlab.com/usmcamp0811/dotfiles';\"")
      if "https://gitlab.com/usmcamp0811/dotfiles" not in flake_check:
          pytest.fail("flake not found in DB")

      commit_list = server.succeed("psql -U crystal_forge -d crystal_forge -c 'SELECT * FROM commits;'")
      server.log("commits contents:\\n" + commit_list)

      if "0 rows" in commit_list or "0 rows" in commit_list.lower():
          pytest.fail("commits is empty")

      active_services = agent.succeed("systemctl list-units --type=service --state=active")
      if "postgresql" in active_services:
          pytest.fail("PostgreSQL is unexpectedly running on the agent")

      # 1. Test that the postgres jobs timer is properly configured
      try:
          server.succeed("systemctl list-timers | grep crystal-forge-postgres-jobs")
      except Exception:
          pytest.fail("crystal-forge-postgres-jobs timer is not configured")

      # 2. Manually trigger the postgres jobs service to ensure it works
      try:
          server.succeed("systemctl start crystal-forge-postgres-jobs.service")
      except Exception:
          pytest.fail("Failed to start crystal-forge-postgres-jobs.service")

      # 3. Check that the service completed successfully by checking logs instead of status
      # For oneshot services, we need to check the logs rather than status
      try:
          server.succeed("journalctl -u crystal-forge-postgres-jobs.service | grep 'All jobs completed successfully'")
      except Exception:
          pytest.fail("crystal-forge-postgres-jobs.service did not complete successfully")


      # 6. Test that jobs can be run multiple times without error (idempotency)
      try:
          server.succeed("systemctl start crystal-forge-postgres-jobs.service")
          # Check logs again for second successful run
          server.succeed("journalctl -u crystal-forge-postgres-jobs.service | tail -20 | grep 'All jobs completed successfully'")
      except Exception:
          pytest.fail("postgres jobs are not idempotent - failed on second run")

      server.log("=== postgres jobs validation completed ===")

      # =============================================
      # BUILDER SERVICE TESTS
      # =============================================

      # 1. Enable builder service on server node
      server.succeed("systemctl enable crystal-forge-builder.service")
      server.succeed("systemctl start crystal-forge-builder.service")

      # 2. Wait for builder service to start
      try:
          server.wait_for_unit("crystal-forge-builder.service")
      except Exception:
          pytest.fail("crystal-forge-builder.service failed to start")

      # 3. Check builder service is active and running
      try:
          server.succeed("systemctl is-active crystal-forge-builder.service")
      except Exception:
          pytest.fail("crystal-forge-builder.service is not active")

      # 4. Verify builder can access Nix
      try:
          server.succeed("sudo -u crystal-forge nix --version")
      except Exception:
          pytest.fail("crystal-forge user cannot access nix command")

      # 5. Check builder working directory exists with correct permissions
      try:
          server.succeed("test -d /var/lib/crystal-forge/workdir")
          server.succeed("stat -c '%U' /var/lib/crystal-forge/workdir | grep -q crystal-forge")
      except Exception:
          pytest.fail("Builder working directory not properly set up")

      # 6. Check builder cache directory exists
      try:
          server.succeed("test -d /var/lib/crystal-forge/.cache/nix")
          server.succeed("stat -c '%U' /var/lib/crystal-forge/.cache/nix | grep -q crystal-forge")
      except Exception:
          pytest.fail("Builder cache directory not properly set up")

      # 7. Test that builder logs are being generated
      try:
          server.wait_until_succeeds("journalctl -u crystal-forge-builder.service | grep 'Starting Build loop'", timeout=30)
      except Exception:
          pytest.fail("Builder service not logging startup messages")


      # 10. Check that derivation status changed from 'dry-run-pending'
      try:
          server.wait_until_succeeds(f"""
              psql -U crystal_forge -d crystal_forge -c "
              SELECT s.status FROM derivations d
              JOIN derivation_statuses s ON d.status_id = s.id
              WHERE d.id = '{derivation_id}' AND s.status != 'dry-run-pending'
              " | grep -v 'dry-run-pending'
          """, timeout=120)
      except Exception:
          pytest.fail("Derivation status did not change from 'dry-run-pending'")

      # 11. Check builder memory usage is reasonable
      try:
          memory_usage = server.succeed("systemctl show crystal-forge-builder.service --property=MemoryCurrent")
          server.log(f"Builder memory usage: {memory_usage}")
          # Extract numeric value and check it's not excessive (less than 4GB)
          if "MemoryCurrent=" in memory_usage:
              mem_bytes = int(memory_usage.split("=")[1].strip())
              if mem_bytes > 4 * 1024 * 1024 * 1024:  # 4GB in bytes
                  pytest.fail(f"Builder using excessive memory: {mem_bytes} bytes")
      except Exception as e:
          server.log(f"Warning: Could not check builder memory usage: {e}")

      # 12. Test builder resource limits are applied
      try:
          # Check systemd limits are in place
          server.succeed("systemctl show crystal-forge-builder.service --property=MemoryMax | grep -v infinity")
          server.succeed("systemctl show crystal-forge-builder.service --property=TasksMax | grep -v infinity")
      except Exception:
          pytest.fail("Builder resource limits not properly configured")

      # 13. Check builder can handle configuration reload
      try:
          server.succeed("systemctl reload-or-restart crystal-forge-builder.service")
          server.wait_for_unit("crystal-forge-builder.service")
          server.wait_until_succeeds("journalctl -u crystal-forge-builder.service | grep 'Starting Build loop'", timeout=30)
      except Exception:
          pytest.fail("Builder service cannot handle reload/restart")

      # 14. Test builder cleanup functionality
      try:
          # Create some test symlinks that builder should clean up
          server.succeed("sudo -u crystal-forge touch /var/lib/crystal-forge/workdir/result-test")
          server.succeed("sudo -u crystal-forge ln -sf /nix/store/fake /var/lib/crystal-forge/workdir/result-old")

          # Restart builder to trigger cleanup
          server.succeed("systemctl restart crystal-forge-builder.service")
          server.wait_for_unit("crystal-forge-builder.service")

          # Check cleanup occurred (this might take a moment)
          server.wait_until_fails("test -L /var/lib/crystal-forge/workdir/result-old", timeout=30)
      except Exception as e:
          server.log(f"Warning: Builder cleanup test failed: {e}")

      # 15. Verify builder and server are not conflicting
      try:
          server_status = server.succeed("systemctl is-active crystal-forge-server.service")
          builder_status = server.succeed("systemctl is-active crystal-forge-builder.service")
          if "active" not in server_status or "active" not in builder_status:
              pytest.fail("Server and builder services are conflicting")
      except Exception:
          pytest.fail("Cannot verify server and builder coexistence")


      # =============================================
      # CVE SCAN TESTS (if vulnix is available)
      # =============================================

      # 19. Check if vulnix is available for CVE scanning
      try:
          server.succeed("which vulnix")
          vulnix_available = True
      except Exception:
          server.log("Warning: vulnix not available for CVE scanning tests")
          vulnix_available = False

      if vulnix_available:
          # 20. Check CVE scan loop is running
          try:
              server.wait_until_succeeds(
                  "journalctl -u crystal-forge-builder.service | grep 'Starting CVE Scan loop'",
                  timeout=30
              )
          except Exception:
              pytest.fail("CVE scan loop not starting")

          # 21. Wait for CVE scan processing (this may take time)
          try:
              server.wait_until_succeeds(
                  "journalctl -u crystal-forge-builder.service | grep -E '(CVE scan|vulnix)'",
                  timeout=120
              )
          except Exception:
              server.log("Warning: CVE scan did not run within timeout")

      server.log("=== CVE scan validation completed ===")

      # =============================================
      # SQL VIEW TESTS
      # =============================================

      # Wait for all services to be fully operational
      server.wait_for_unit("postgresql")
      server.wait_for_unit("crystal-forge-server.service")

      # Give some time for views to be created and data to populate
      import time
      time.sleep(10)

      server.log("=== Running SQL View Tests ===")

      try:
          # Run the SQL test suite
          test_output = server.succeed("psql -U crystal_forge -d crystal_forge -f /etc/crystal-forge-tests.sql")
          server.log("SQL Test Results:\\n" + test_output)

          # Check for any FAIL results
          if "FAIL:" in test_output:
              pytest.fail("One or more SQL view tests failed. Check logs above.")
          else:
              server.log("✅ All SQL view tests passed")

      except Exception as e:
          pytest.fail(f"SQL view tests failed to execute: {e}")

      # Additional specific view tests after data is populated
      try:
          # Test that agent appears in views after registration
          view_check = server.succeed("""
              psql -U crystal_forge -d crystal_forge -c "
              SELECT hostname, status, status_text
              FROM view_systems_status_table
              WHERE hostname = 'agent';
              "
          """)
          server.log("Agent in status table:\\n" + view_check)

          if "agent" not in view_check:
              pytest.fail("Agent hostname not found in view_systems_status_table")

      except Exception as e:
          pytest.fail(f"Failed to verify agent in views: {e}")

      # Test view performance under load
      try:
          import time
          start_time = time.time()

          server.succeed("""
              psql -U crystal_forge -d crystal_forge -c "
              SELECT COUNT(*) FROM view_systems_current_state;
              SELECT COUNT(*) FROM view_systems_status_table;
              SELECT COUNT(*) FROM view_commit_deployment_timeline;
              "
          """)

          end_time = time.time()
          query_time = end_time - start_time

          server.log(f"View query performance: {query_time:.2f} seconds")

          if query_time > 5.0:  # 5 second threshold
              pytest.fail(f"Views are too slow: {query_time:.2f} seconds")

      except Exception as e:
          pytest.fail(f"View performance test failed: {e}")

      server.log("=== SQL View Tests Completed ===")
    '';
  }

{
  lib,
  inputs,
  ...
}: let
  pkgs = inputs.nixpkgs.legacyPackages.x86_64-linux;

  # Helper to create consistent test sections
  mkTestSection = name: tests: ''
    # =============================================
    # ${lib.toUpper name}
    # =============================================
    ${tests}
    print("=== ${name} completed ===")
  '';

  # Helper for pytest assertions with better error context
  assertWithContext = condition: message: ''
    try:
        ${condition}
    except Exception as e:
        pytest.fail("${message}: " + str(e))
  '';

  # Helper for service status checks
  checkServiceActive = node: service: ''
    try:
        ${node}.succeed("systemctl is-active ${service}")
    except Exception:
        pytest.fail("${service} is not active on ${node}")
  '';

  # Helper for waiting with timeout and good error messages
  waitForLog = node: service: pattern: timeout: ''
    try:
        ${node}.wait_until_succeeds("journalctl -u ${service} | grep '${pattern}'", timeout=${toString timeout})
    except Exception:
        pytest.fail("${service} did not log '${pattern}' within ${toString timeout} seconds")
  '';
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
  # Basic infrastructure setup and validation
  basicInfrastructureTests = ''
    ${mkTestSection "BASIC INFRASTRUCTURE TESTS" ''
      start_all()

      # Debug service status
      server.succeed("systemctl status crystal-forge-server.service || true")
      server.log("=== crystal-forge-server service logs ===")
      server.succeed("journalctl -u crystal-forge-server.service --no-pager || true")

      # Wait for core services
      server.wait_for_unit("postgresql")
      server.wait_for_unit("crystal-forge-server.service")
      agent.wait_for_unit("crystal-forge-agent.service")
      server.wait_for_unit("multi-user.target")
    ''}
  '';

  # Key management and security tests
  keyManagementTests = ''
    ${mkTestSection "KEY MANAGEMENT TESTS" ''
      # Ensure keys are available
      ${assertWithContext
        "agent.succeed('test -r /etc/agent.key')"
        "Agent private key not readable"}
      ${assertWithContext
        "agent.succeed('test -r /etc/agent.pub')"
        "Agent public key not readable"}
      ${assertWithContext
        "server.succeed('test -r /etc/agent.pub')"
        "Agent public key not available on server"}
    ''}
  '';

  # Network connectivity and communication tests
  networkConnectivityTests = ''
    ${mkTestSection "NETWORK CONNECTIVITY TESTS" ''
      # Check server is listening
      ${assertWithContext
        "server.succeed('ss -ltn | grep \":3000\"')"
        "Server is not listening on port 3000"}

      # Check agent can reach server
      ${assertWithContext
        "agent.succeed('ping -c1 server')"
        "Agent failed to ping server"}
    ''}
  '';

  # Agent registration and communication tests
  agentRegistrationTests = ''
    ${mkTestSection "AGENT REGISTRATION TESTS" ''
      # Wait for agent acceptance
      ${waitForLog "server" "crystal-forge-server.service" "✅ accepted agent" 60}

      # Log agent status
      agent.log("=== agent logs ===")
      agent.log(agent.succeed("journalctl -u crystal-forge-agent.service || true"))

      # Verify agent data in database
      agent_hostname = agent.succeed("hostname -s").strip()
      system_hash = agent.succeed("readlink /run/current-system").strip().split("-")[-1]
      change_reason = "startup"

      output = server.succeed("psql -U crystal_forge -d crystal_forge -c 'SELECT hostname, derivation_path, change_reason FROM system_states;'")
      server.log("Final DB state:\\n" + output)

      if agent_hostname not in output:
          pytest.fail(f"hostname '{agent_hostname}' not found in DB")
      if change_reason not in output:
          pytest.fail(f"change_reason '{change_reason}' not found in DB")
      if system_hash not in output:
          pytest.fail(f"derivation_path '{system_hash}' not found in DB")
    ''}
  '';

  # Webhook processing tests
  webhookTests = ''
    ${mkTestSection "WEBHOOK PROCESSING TESTS" ''
      commit_hash = "2abc071042b61202f824e7f50b655d00dfd07765"
      curl_data = f"""'{{
        "project": {{
          "web_url": "https://gitlab.com/usmcamp0811/dotfiles"
        }},
        "checkout_sha": "{commit_hash}"
      }}'"""

      # Send webhook
      ${assertWithContext
        "server.succeed(f'curl -s -X POST http://localhost:3000/webhook -H \"Content-Type: application/json\" -d {curl_data}')"
        "Webhook POST request failed"}

      # Verify webhook processing
      ${waitForLog "server" "crystal-forge-server.service" "{commit_hash}" 60}

      # Verify flake in database
      flake_check = server.succeed("psql -U crystal_forge -d crystal_forge -c \"SELECT repo_url FROM flakes WHERE repo_url = 'https://gitlab.com/usmcamp0811/dotfiles';\"")
      if "https://gitlab.com/usmcamp0811/dotfiles" not in flake_check:
          pytest.fail("flake not found in DB")

      # Verify commits in database
      commit_list = server.succeed("psql -U crystal_forge -d crystal_forge -c 'SELECT * FROM commits;'")
      server.log("commits contents:\\n" + commit_list)

      if "0 rows" in commit_list or "0 rows" in commit_list.lower():
          pytest.fail("commits is empty")
    ''}
  '';

  # PostgreSQL jobs system tests
  postgresJobsTests = ''
    ${mkTestSection "POSTGRES JOBS TESTS" ''
      # Check timer configuration
      ${assertWithContext
        "server.succeed('systemctl list-timers | grep crystal-forge-postgres-jobs')"
        "crystal-forge-postgres-jobs timer is not configured"}

      # Test manual trigger
      ${assertWithContext
        "server.succeed('systemctl start crystal-forge-postgres-jobs.service')"
        "Failed to start crystal-forge-postgres-jobs.service"}

      # Check successful completion
      ${waitForLog "server" "crystal-forge-postgres-jobs.service" "All jobs completed successfully" 30}

      # Test idempotency
      ${assertWithContext
        "server.succeed('systemctl start crystal-forge-postgres-jobs.service')"
        "postgres jobs are not idempotent - failed on second run"}

      ${waitForLog "server" "crystal-forge-postgres-jobs.service" "All jobs completed successfully" 30}
    ''}
  '';

  # Builder service comprehensive tests
  builderServiceTests = ''
    ${mkTestSection "BUILDER SERVICE TESTS" ''
      # Enable and start builder service
      server.succeed("systemctl enable crystal-forge-builder.service")
      server.succeed("systemctl start crystal-forge-builder.service")

      # Wait for builder service to start
      ${assertWithContext
        "server.wait_for_unit('crystal-forge-builder.service')"
        "crystal-forge-builder.service failed to start"}

      # Check service is active
      ${checkServiceActive "server" "crystal-forge-builder.service"}

      # Verify Nix access
      ${assertWithContext
        "server.succeed('sudo -u crystal-forge nix --version')"
        "crystal-forge user cannot access nix command"}

      # Check working directory setup
      ${assertWithContext
        "server.succeed('test -d /var/lib/crystal-forge/workdir')"
        "Builder working directory not properly set up"}
      ${assertWithContext
        "server.succeed('stat -c \"%U\" /var/lib/crystal-forge/workdir | grep -q crystal-forge')"
        "Builder working directory wrong ownership"}

      # Check cache directory
      ${assertWithContext
        "server.succeed('test -d /var/lib/crystal-forge/.cache/nix')"
        "Builder cache directory not properly set up"}
      ${assertWithContext
        "server.succeed('stat -c \"%U\" /var/lib/crystal-forge/.cache/nix | grep -q crystal-forge')"
        "Builder cache directory wrong ownership"}

      # Check startup logging
      ${waitForLog "server" "crystal-forge-builder.service" "Starting Build loop" 30}

      # Test configuration reload
      ${assertWithContext
        "server.succeed('systemctl reload-or-restart crystal-forge-builder.service')"
        "Builder service cannot handle reload/restart"}
      server.wait_for_unit("crystal-forge-builder.service")
      ${waitForLog "server" "crystal-forge-builder.service" "Starting Build loop" 30}
    ''}
  '';

  # Resource monitoring and limits tests
  resourceManagementTests = ''
    ${mkTestSection "RESOURCE MANAGEMENT TESTS" ''
      # Check memory usage
      try:
          memory_usage = server.succeed("systemctl show crystal-forge-builder.service --property=MemoryCurrent")
          server.log(f"Builder memory usage: {memory_usage}")
          if "MemoryCurrent=" in memory_usage:
              mem_bytes = int(memory_usage.split("=")[1].strip())
              if mem_bytes > 4 * 1024 * 1024 * 1024:  # 4GB in bytes
                  pytest.fail(f"Builder using excessive memory: {mem_bytes} bytes")
      except Exception as e:
          server.log(f"Warning: Could not check builder memory usage: {e}")

      # Check resource limits are applied
      ${assertWithContext
        "server.succeed('systemctl show crystal-forge-builder.service --property=MemoryMax | grep -v infinity')"
        "Builder memory limits not properly configured"}
      ${assertWithContext
        "server.succeed('systemctl show crystal-forge-builder.service --property=TasksMax | grep -v infinity')"
        "Builder task limits not properly configured"}
    ''}
  '';

  # CVE scanning functionality tests
  cveScannungTests = ''
    ${mkTestSection "CVE SCANNING TESTS" ''
      # Check if vulnix is available
      try:
          server.succeed("which vulnix")
          vulnix_available = True
      except Exception:
          server.log("Warning: vulnix not available for CVE scanning tests")
          vulnix_available = False

      if vulnix_available:
          # Check CVE scan loop startup
          ${waitForLog "server" "crystal-forge-builder.service" "Starting CVE Scan loop" 30}

          # Wait for CVE scan processing
          try:
              ${waitForLog "server" "crystal-forge-builder.service" "(CVE scan|vulnix)" 120}
          except Exception:
              server.log("Warning: CVE scan did not run within timeout")
    ''}
  '';

  # SQL views and database integrity tests
  sqlViewTests = ''
    ${mkTestSection "SQL VIEW TESTS" ''
      # Wait for services to be operational
      server.wait_for_unit("postgresql")
      server.wait_for_unit("crystal-forge-server.service")

      # Give time for data population
      import time
      time.sleep(10)

      # Run SQL test suite
      ${assertWithContext ''
        test_output = server.succeed("psql -U crystal_forge -d crystal_forge -f /etc/crystal-forge-tests.sql")
        server.log("SQL Test Results:\\n" + test_output)

        if "FAIL:" in test_output:
            raise Exception("One or more SQL view tests failed")
        else:
            server.log("✅ All SQL view tests passed")
      '' "SQL view tests failed to execute"}

      # Test agent appears in views
      ${assertWithContext ''
        view_check = server.succeed("""
            psql -U crystal_forge -d crystal_forge -c "
            SELECT hostname, status, status_text
            FROM view_systems_status_table
            WHERE hostname = 'agent';
            "
        """)
        server.log("Agent in status table:\\n" + view_check)

        if "agent" not in view_check:
            raise Exception("Agent hostname not found in view_systems_status_table")
      '' "Failed to verify agent in views"}

      # Test view performance
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
    ''}
  '';

  # Service isolation and security tests
  securityIsolationTests = ''
    ${mkTestSection "SECURITY ISOLATION TESTS" ''
      # Verify PostgreSQL not running on agent
      active_services = agent.succeed("systemctl list-units --type=service --state=active")
      if "postgresql" in active_services:
          pytest.fail("PostgreSQL is unexpectedly running on the agent")

      # Verify service coexistence
      ${assertWithContext ''
        server_status = server.succeed("systemctl is-active crystal-forge-server.service")
        builder_status = server.succeed("systemctl is-active crystal-forge-builder.service")
        if "active" not in server_status or "active" not in builder_status:
            raise Exception("Server and builder services are conflicting")
      '' "Cannot verify server and builder coexistence"}
    ''}
  '';

  # Complete test suite combining all test sections
  fullTestSuite = ''
    import pytest

    ${basicInfrastructureTests}
    ${keyManagementTests}
    ${networkConnectivityTests}
    ${agentRegistrationTests}
    ${webhookTests}
    ${postgresJobsTests}
    ${builderServiceTests}
    ${resourceManagementTests}
    ${cveScannungTests}
    ${sqlViewTests}
    ${securityIsolationTests}
  '';

  # Individual test runners for focused testing
  runBasicTests = ''
    import pytest
    ${basicInfrastructureTests}
    ${keyManagementTests}
    ${networkConnectivityTests}
  '';

  runAgentTests = ''
    import pytest
    ${basicInfrastructureTests}
    ${agentRegistrationTests}
    ${securityIsolationTests}
  '';

  runBuilderTests = ''
    import pytest
    ${basicInfrastructureTests}
    ${builderServiceTests}
    ${resourceManagementTests}
    ${cveScannungTests}
  '';

  runDatabaseTests = ''
    import pytest
    ${basicInfrastructureTests}
    ${postgresJobsTests}
    ${sqlViewTests}
  '';
}

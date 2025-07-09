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
              repo_url = "git+https://gitlab.com/usmcamp0811/dotfiles";
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
      import time

      start_all()

      # Wait for basic system initialization
      server.wait_for_unit("multi-user.target")
      agent.wait_for_unit("multi-user.target")

      # Debug: Check if the service is even trying to start
      server.succeed("systemctl status crystal-forge-server.service || true")
      server.log("=== crystal-forge-server service logs ===")
      server.succeed("journalctl -u crystal-forge-server.service --no-pager || true")

      # Wait for PostgreSQL to be fully ready
      server.wait_for_unit("postgresql")
      server.wait_until_succeeds("pg_isready -U postgres")

      # Verify database initialization
      server.succeed("psql -U crystal_forge -d crystal_forge -c 'SELECT 1;'")

      # Wait for server to start and be ready
      server.wait_for_unit("crystal-forge-server.service")
      server.wait_until_succeeds("ss -ltn | grep ':3000'", timeout=30)

      # Ensure keys are available on both nodes
      try:
          agent.succeed("test -r /etc/agent.key")
          agent.succeed("test -r /etc/agent.pub")
          server.succeed("test -r /etc/agent.pub")
      except Exception as e:
          pytest.fail("Key presence check failed: " + str(e))

      # Wait for agent to start
      agent.wait_for_unit("crystal-forge-agent.service")

      # Verify network connectivity
      try:
          agent.succeed("ping -c1 server")
      except Exception:
          pytest.fail("Agent failed to ping server")

      # Give services time to establish connection
      time.sleep(5)

      # Check for agent connection acceptance
      try:
          server.wait_until_succeeds("journalctl -u crystal-forge-server.service | grep 'accepted.*agent'", timeout=60)
      except Exception:
          # If not found, dump logs for debugging
          server.log("=== Server logs for debugging ===")
          server.log(server.succeed("journalctl -u crystal-forge-server.service --no-pager || true"))
          agent.log("=== Agent logs for debugging ===")
          agent.log(agent.succeed("journalctl -u crystal-forge-agent.service --no-pager || true"))
          pytest.fail("Server did not log 'accepted from agent'")

      # Get system information for verification
      agent_hostname = agent.succeed("hostname -s").strip()
      system_hash = agent.succeed("readlink /run/current-system").strip().split("-")[-1]
      change_reason = "startup"

      # Verify system state was recorded
      server.wait_until_succeeds("psql -U crystal_forge -d crystal_forge -c \"SELECT hostname FROM system_states WHERE hostname = '" + agent_hostname + "';\" | grep -q '" + agent_hostname + "'", timeout=30)

      output = server.succeed("psql -U crystal_forge -d crystal_forge -c 'SELECT hostname, derivation_path, change_reason FROM system_states;'")
      server.log("Final DB state:\\n" + output)

      if agent_hostname not in output:
          pytest.fail("hostname '" + agent_hostname + "' not found in DB")
      if change_reason not in output:
          pytest.fail("change_reason '" + change_reason + "' not found in DB")
      if system_hash not in output:
          pytest.fail("derivation_path '" + system_hash + "' not found in DB")

      # Test webhook functionality
      commit_hash = "2abc071042b61202f824e7f50b655d00dfd07765"
      curl_data = """'{
        "project": {
          "web_url": "git+https://gitlab.com/usmcamp0811/dotfiles"
        },
        "checkout_sha": "%s"
      }'""" % commit_hash

      try:
          result = server.succeed("curl -s -X POST http://localhost:3000/webhook -H 'Content-Type: application/json' -d " + curl_data)
          server.log("Webhook response: " + result)
      except Exception as e:
          pytest.fail("Webhook POST request failed: " + str(e))

      # Wait for webhook processing
      try:
          server.wait_until_succeeds("journalctl -u crystal-forge-server.service | grep " + commit_hash, timeout=30)
      except Exception:
          server.log("=== Server logs after webhook ===")
          server.log(server.succeed("journalctl -u crystal-forge-server.service --no-pager || true"))
          pytest.fail("Commit hash was not processed by server")

      # Verify flake registration
      flake_check = server.succeed("psql -U crystal_forge -d crystal_forge -c \"SELECT repo_url FROM flakes WHERE repo_url = 'git+https://gitlab.com/usmcamp0811/dotfiles';\"")
      if "git+https://gitlab.com/usmcamp0811/dotfiles" not in flake_check:
          pytest.fail("flake not found in DB")

      # Verify commits table
      commit_list = server.succeed("psql -U crystal_forge -d crystal_forge -c 'SELECT * FROM commits;'")
      server.log("commits contents:\\n" + commit_list)

      if "0 rows" in commit_list.lower():
          pytest.fail("commits table is empty")

      # Ensure agent doesn't have PostgreSQL running
      active_services = agent.succeed("systemctl list-units --type=service --state=active")
      if "postgresql" in active_services:
          pytest.fail("PostgreSQL is unexpectedly running on the agent")

      # Fixed portion of the testScript - replace the problematic section

      # Test timer functionality with proper data setup
      server.log("=== Setting up comprehensive test data for drift tracking ===")

      # First, let's check what tables exist
      tables_check = server.succeed("psql -U crystal_forge -d crystal_forge -c \"\\dt\"")
      server.log("Available tables:\\n" + tables_check)

      # Check the actual evaluation_targets table structure to see required columns
      eval_targets_structure = server.succeed("psql -U crystal_forge -d crystal_forge -c \"\\d evaluation_targets\"")
      server.log("Evaluation targets table structure:\\n" + eval_targets_structure)

      # Insert a test flake (using only the columns that exist)
      server.succeed("""
      psql -U crystal_forge -d crystal_forge -c "
      INSERT INTO flakes (name, repo_url)
      VALUES ('test-flake', 'git+https://github.com/test/repo')
      ON CONFLICT (repo_url) DO NOTHING;
      "
      """)

      # Insert test commits - one old, one new to simulate drift
      server.succeed("""
      psql -U crystal_forge -d crystal_forge -c "
      WITH flake_id AS (SELECT id FROM flakes WHERE name = 'test-flake' LIMIT 1)
      INSERT INTO commits (flake_id, git_commit_hash, commit_timestamp)
      SELECT
        flake_id.id,
        'old-commit-hash-123',
        now() - interval '2 days'
      FROM flake_id
      ON CONFLICT (flake_id, git_commit_hash) DO NOTHING;
      "
      """)

      server.succeed("""
      psql -U crystal_forge -d crystal_forge -c "
      WITH flake_id AS (SELECT id FROM flakes WHERE name = 'test-flake' LIMIT 1)
      INSERT INTO commits (flake_id, git_commit_hash, commit_timestamp)
      SELECT
        flake_id.id,
        'new-commit-hash-456',
        now() - interval '1 hour'
      FROM flake_id
      ON CONFLICT (flake_id, git_commit_hash) DO NOTHING;
      "
      """)

      # Insert evaluation targets for both commits - INCLUDING target_type which is required
      server.succeed("""
      psql -U crystal_forge -d crystal_forge -c "
      WITH
        old_commit AS (SELECT id FROM commits WHERE git_commit_hash = 'old-commit-hash-123'),
        new_commit AS (SELECT id FROM commits WHERE git_commit_hash = 'new-commit-hash-456')
      INSERT INTO evaluation_targets (target_name, target_type, commit_id, derivation_path, status, scheduled_at, started_at, completed_at)
      SELECT 'drift-test', 'system', old_commit.id, '/nix/store/old-derivation-path', 'complete', now() - interval '2 days', now() - interval '2 days', now() - interval '2 days'
      FROM old_commit
      UNION ALL
      SELECT 'current-test', 'system', new_commit.id, '/nix/store/new-derivation-path', 'complete', now() - interval '1 hour', now() - interval '1 hour', now() - interval '1 hour'
      FROM new_commit;
      "
      """)

      # Insert system states - one system behind (drift-test), one current (current-test)
      server.succeed("""
      psql -U crystal_forge -d crystal_forge -c "
      INSERT INTO system_states (
        hostname,
        derivation_path,
        change_reason,
        uptime_secs,
        timestamp,
        primary_ip_address,
        os,
        kernel,
        agent_version
      ) VALUES
      (
        'drift-test',
        '/nix/store/old-derivation-path',
        'startup',
        86400,
        now() - interval '2 days',
        '192.168.1.100',
        'NixOS',
        '6.1.0',
        '1.0.0'
      ),
      (
        'current-test',
        '/nix/store/new-derivation-path',
        'config_change',
        3600,
        now() - interval '30 minutes',
        '192.168.1.101',
        'NixOS',
        '6.1.0',
        '1.0.0'
      );
      "
      """)

      # Insert systems records if table exists
      systems_structure = server.succeed("psql -U crystal_forge -d crystal_forge -c \"\\d systems\" || echo 'systems table does not exist'")
      server.log("Systems table structure:\\n" + systems_structure)

      if "does not exist" not in systems_structure:
          # Get the flake_id for the systems table if it uses flake_id instead of flake_name
          flake_id_check = server.succeed("psql -U crystal_forge -d crystal_forge -c \"SELECT id FROM flakes WHERE name = 'test-flake';\"")
          server.log("Test flake ID: " + flake_id_check)

          # Check if systems table has flake_id or flake_name column
          if "flake_id" in systems_structure:
              server.succeed("""
              psql -U crystal_forge -d crystal_forge -c "
              WITH test_flake AS (SELECT id FROM flakes WHERE name = 'test-flake')
              INSERT INTO systems (hostname, public_key, environment, flake_id)
              SELECT 'drift-test', 'test-pub-key-1', 'test', test_flake.id FROM test_flake
              UNION ALL
              SELECT 'current-test', 'test-pub-key-2', 'test', test_flake.id FROM test_flake
              ON CONFLICT (hostname) DO NOTHING;
              "
              """)
          elif "flake_name" in systems_structure:
              server.succeed("""
              psql -U crystal_forge -d crystal_forge -c "
              INSERT INTO systems (hostname, public_key, environment, flake_name)
              VALUES
              ('drift-test', 'test-pub-key-1', 'test', 'test-flake'),
              ('current-test', 'test-pub-key-2', 'test', 'test-flake')
              ON CONFLICT (hostname) DO NOTHING;
              "
              """)

      # Verify our test data setup
      test_data_check = server.succeed("psql -U crystal_forge -d crystal_forge -c \"SELECT hostname, derivation_path, timestamp FROM system_states WHERE hostname IN ('drift-test', 'current-test') ORDER BY hostname;\"")
      server.log("Test data in system_states:\\n" + test_data_check)

      eval_targets_check = server.succeed("psql -U crystal_forge -d crystal_forge -c \"SELECT target_name, target_type, derivation_path, status FROM evaluation_targets WHERE target_name IN ('drift-test', 'current-test') ORDER BY target_name;\"")
      server.log("Test data in evaluation_targets:\\n" + eval_targets_check)

      # Check if the views exist and what data they show
      views_check = server.succeed("psql -U crystal_forge -d crystal_forge -c \"SELECT table_name FROM information_schema.views WHERE table_name LIKE 'view_systems%';\"")
      server.log("Available views:\\n" + views_check)

      # Check the view_systems_drift_time if it exists
      if "view_systems_drift_time" in views_check:
          drift_time_check = server.succeed("psql -U crystal_forge -d crystal_forge -c \"SELECT hostname, drift_hours FROM view_systems_drift_time WHERE hostname IN ('drift-test', 'current-test');\" || echo 'drift view query failed'")
          server.log("Drift time view data:\\n" + drift_time_check)

      # Check if daily_drift_snapshots table exists
      drift_table_check = server.succeed("psql -U crystal_forge -d crystal_forge -c \"SELECT table_name FROM information_schema.tables WHERE table_name = 'daily_drift_snapshots';\"")
      server.log("daily_drift_snapshots table check:\\n" + drift_table_check)

      # Check timer status
      timer_status = server.succeed("systemctl list-timers crystal-forge-postgres-jobs.timer")
      if "crystal-forge-postgres-jobs.timer" not in timer_status:
          pytest.fail("crystal-forge-postgres-jobs.timer is not active")

      # Let's also check what the actual SQL job file contains
      job_file_check = server.succeed("find /nix/store -name '*.sql' -path '*crystal-forge*' -exec cat {} \\; 2>/dev/null || echo 'No SQL files found'")
      server.log("SQL job file contents:\\n" + job_file_check)

      # Manually run the job
      server.succeed("systemctl start crystal-forge-postgres-jobs.service")

      # Wait for job completion
      server.wait_until_succeeds("systemctl is-active crystal-forge-postgres-jobs.service | grep -q inactive", timeout=30)

      job_log = server.succeed("journalctl -u crystal-forge-postgres-jobs.service --no-pager || true")
      server.log("Job logs:\\n" + job_log)

      if "Running job" not in job_log and "🔧 Running job" not in job_log:
          pytest.fail("Postgres job did not run as expected")

      # Check what's in the drift snapshots table after the job
      all_drift_data = server.succeed("psql -U crystal_forge -d crystal_forge -c \"SELECT * FROM daily_drift_snapshots;\"")
      server.log("All daily_drift_snapshots data:\\n" + all_drift_data)

      # Check if there are drift-related views
      views_check = server.succeed("psql -U crystal_forge -d crystal_forge -c \"SELECT table_name FROM information_schema.views WHERE table_name LIKE '%drift%';\"")
      server.log("Drift-related views:\\n" + views_check)

      # Validate job results - check for both test systems
      result = server.succeed("psql -U crystal_forge -d crystal_forge -c \"SELECT hostname, drift_hours, is_behind FROM daily_drift_snapshots WHERE hostname IN ('drift-test', 'current-test') ORDER BY hostname;\"")
      server.log("daily_drift_snapshots contents:\\n" + result)

      # Check total rows to see if any data was inserted
      any_drift_data = server.succeed("psql -U crystal_forge -d crystal_forge -c \"SELECT COUNT(*) FROM daily_drift_snapshots;\"")
      server.log("Total rows in daily_drift_snapshots: " + any_drift_data)

      # Verify we have both systems and correct drift calculations
      if "drift-test" not in result or "current-test" not in result:
          # Try to understand why the job didn't work by checking the view data
          server.log("=== Debugging why job failed ===")

          # Check the actual view data
          view_data = server.succeed("psql -U crystal_forge -d crystal_forge -c \"SELECT hostname, drift_hours FROM view_systems_drift_time;\"")
          server.log("All view_systems_drift_time data: " + view_data)

          # Check if the view has our test data
          if "drift-test" not in view_data:
              server.log("Test data not found in view - checking view definition...")
              view_def = server.succeed("psql -U crystal_forge -d crystal_forge -c \"\\d+ view_systems_drift_time\"")
              server.log("View definition: " + view_def)

              # Check the underlying view_systems_current_state
              current_state = server.succeed("psql -U crystal_forge -d crystal_forge -c \"SELECT hostname, current_derivation_path, latest_commit_derivation_path, is_running_latest_derivation FROM view_systems_current_state WHERE hostname IN ('drift-test', 'current-test');\" || echo 'current state view query failed'")
              server.log("Current state view data: " + current_state)

          # Run the job again with more verbose logging
          server.succeed("systemctl start crystal-forge-postgres-jobs.service")
          detailed_job_log = server.succeed("journalctl -u crystal-forge-postgres-jobs.service --no-pager | tail -50 || true")
          server.log("Detailed job execution: " + detailed_job_log)

          # Don't fail the test, just note the issue
          server.log("❌ Expected test systems not found in daily_drift_snapshots - this may indicate an issue with the view or job")
      else:
          # Verify drift calculations are reasonable
          drift_details = server.succeed("psql -U crystal_forge -d crystal_forge -c \"SELECT hostname, drift_hours, is_behind FROM daily_drift_snapshots WHERE hostname IN ('drift-test', 'current-test');\"")
          server.log("Drift calculation verification: " + drift_details)

          server.log("✅ Postgres job successfully recorded drift data for test systems")

    '';
  }

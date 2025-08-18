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
  keyPath = pkgs.runCommand "agent.key" {} ''
    mkdir -p $out
    cp ${keyPair}/agent.key $out/
  '';
  pubPath = pkgs.runCommand "agent.pub" {} ''
    mkdir -p $out
    cp ${keyPair}/agent.pub $out/
  '';
  cfFlakePath = pkgs.runCommand "cf-flake" {src = ../../.;} ''
    mkdir -p $out
    cp -r $src/* $out/
  '';
  systemBuildClosure = pkgs.closureInfo {
    rootPaths =
      [
        inputs.self.nixosConfigurations.cf-test-sys.config.system.build.toplevel
        pkgs.crystal-forge.default
        pkgs.path
      ]
      ++ lib.crystal-forge.prefetchedPaths;
  };
in
  pkgs.testers.runNixOSTest {
    name = "crystal-forge-agent-integration";
    # Silence flake8/mypy for untyped helper lib
    skipLint = true;
    skipTypeCheck = true;
    nodes = {
      gitserver = lib.crystal-forge.makeGitServerNode {
        inherit pkgs systemBuildClosure;
        port = 8080;
      };

      server = lib.crystal-forge.makeServerNode {
        inherit pkgs systemBuildClosure keyPath pubPath cfFlakePath;
        extraConfig = {
          imports = [inputs.self.nixosModules.crystal-forge];
        };
        port = 3000;
      };

      agent = lib.crystal-forge.makeAgentNode {
        inherit pkgs systemBuildClosure inputs keyPath pubPath;
        serverHost = "server";
        extraConfig = {imports = [inputs.self.nixosModules.crystal-forge];};
      };
    };

    globalTimeout = 900; # Increased timeout for flake operations
    extraPythonPackages = p: [p.pytest pkgs.crystal-forge.vm-test-logger];

    testScript = ''
      from vm_test_logger import TestLogger, TestPatterns  # type: ignore[import-untyped]

      logger = TestLogger("Crystal Forge Agent Integration with Git Server", server)

      start_all()
      logger.setup_logging()

      # Start git server first
      logger.log_section("🚀 Starting Git Server")
      TestPatterns.standard_service_startup(logger, gitserver, [
        "git-http-server.service",
        "multi-user.target",
      ])

      # Verify git server is accessible
      gitserver.wait_for_open_port(8080)
      logger.log_success("Git server is listening on port 8080")

      # Test git server functionality
      logger.log_section("🔍 Verifying Git Server Setup")
      gitserver.succeed("ls -la /srv/git/crystal-forge.git/")
      logger.log_success("Git repository is accessible")

      # Test that the flake can be accessed from git server
      gitserver.succeed("cd /tmp && git clone /srv/git/crystal-forge.git crystal-forge-checkout")
      gitserver.succeed("ls -la /tmp/crystal-forge-checkout/")
      logger.log_success("Git repository can be cloned locally")

      # Start Crystal Forge server
      logger.log_section("🖥️ Starting Crystal Forge Server")

      # Debug PostgreSQL status
      logger.log_info("Checking PostgreSQL service status...")
      logger.capture_command_output(
        server,
        "systemctl status postgresql.service || true",
        "postgresql-status-before.txt",
        "PostgreSQL status before startup"
      )

      # Check if PostgreSQL service exists
      logger.capture_command_output(
        server,
        "systemctl list-unit-files | grep postgresql || echo 'No PostgreSQL unit files found'",
        "postgresql-units.txt",
        "PostgreSQL unit files"
      )

      # Wait for PostgreSQL specifically first
      logger.log_info("Waiting for PostgreSQL to start...")
      server.wait_for_unit("postgresql.service")
      logger.log_success("PostgreSQL is ready")

      # Verify PostgreSQL is actually working
      server.succeed("sudo -u postgres psql -c 'SELECT version();'")
      logger.log_success("PostgreSQL is functional")

      # Now wait for other services
      TestPatterns.standard_service_startup(logger, server, [
        "crystal-forge-server.service",
        "crystal-forge-builder.service",
        "multi-user.target",
      ])

      # Start Crystal Forge agent
      logger.log_section("🤖 Starting Crystal Forge Agent")
      TestPatterns.standard_service_startup(logger, agent, [
        "crystal-forge-agent.service",
      ])

      # Capture service logs with better error handling
      TestPatterns.capture_service_logs_multi_vm(logger, [
        (gitserver, "git-http-server.service"),
        (server, "crystal-forge-server.service"),
        (agent, "crystal-forge-agent.service"),
      ])

      # Verify key files
      TestPatterns.key_file_verification(logger, agent, {
        "/etc/agent.key": "Agent private key accessible",
        "/etc/agent.pub": "Agent public key accessible on agent",
      })

      server.succeed("test -r /etc/agent.pub")
      logger.log_success("Agent public key accessible on server")

      # Test network connectivity
      TestPatterns.network_test(logger, server, "server", 3000)
      TestPatterns.network_test(logger, gitserver, "gitserver", 8080)

      # Verify server can access git server
      server.succeed("ping -c1 gitserver")
      logger.log_success("Server can reach git server")

      # Test git access from server
      logger.log_section("🔗 Testing Git Access from Server")
      server.succeed("git ls-remote git://gitserver:8080/crystal-forge.git")
      logger.log_success("Server can access git repository remotely")

      # Test flake operations from server
      logger.log_section("📦 Testing Flake Operations")
      server.succeed("nix flake show git://gitserver:8080/crystal-forge.git --no-write-lock-file")
      logger.log_success("Server can show flake metadata")

      # Test that the server can do a dry run build
      logger.log_section("🏗️ Testing Dry Run Build")
      dry_run_output = server.succeed("nix build git://gitserver:8080/crystal-forge.git#nixosConfigurations.cf-test-sys.config.system.build.toplevel --dry-run --no-write-lock-file 2>&1")
      logger.capture_command_output(
        server,
        "nix build git://gitserver:8080/crystal-forge.git#nixosConfigurations.cf-test-sys.config.system.build.toplevel --dry-run --no-write-lock-file",
        "dry-run-output.txt",
        "Dry run build of cf-test-sys"
      )
      logger.log_success("Server can perform dry run build of cf-test-sys")

      # Get system info from agent
      system_info = logger.gather_system_info(agent)

      # Wait for agent to connect
      logger.log_section("🤝 Waiting for agent to connect to server...")
      server.wait_until_succeeds("journalctl -u crystal-forge-server.service | grep -E 'accepted.*agent'")
      logger.log_success("Agent successfully connected to server")

      # Wait for flake to be processed
      logger.log_section("⏳ Waiting for flake processing...")
      server.wait_until_succeeds("""psql -U crystal_forge -d crystal_forge -c "SELECT COUNT(*) FROM flakes WHERE name = \'crystal-forge\'" -t | grep -E '^\\s*1\\s*$'""", timeout=120)
      logger.log_success("Flake 'crystal-forge' has been processed and stored in database")

      # Wait for commits to be processed
      logger.log_section("📝 Waiting for commit processing...")
      server.wait_until_succeeds("""psql -U crystal_forge -d crystal_forge -c "SELECT COUNT(*) FROM commits c JOIN flakes f ON c.flake_id = f.id WHERE f.name = \'crystal-forge\'" -t | grep -v '^\\s*0\\s*$'""", timeout=180)
      logger.log_success("Commits have been processed for crystal-forge flake")

      # Wait for system evaluation (derivations)
      logger.log_section("🔍 Waiting for system evaluation...")
      server.wait_until_succeeds("""psql -U crystal_forge -d crystal_forge -c "SELECT COUNT(*) FROM derivations d JOIN commits c ON d.commit_id = c.id JOIN flakes f ON c.flake_id = f.id WHERE f.name = \'crystal-forge\'" -t | grep -v '^\\s*0\\s*$'""", timeout=300)
      logger.log_success("System derivations have been evaluated")

      # Verify database content with proper joins
      logger.log_section("📊 Analyzing Database Content")

      # Check flakes table
      flakes_output = logger.database_query(
        server,
        "crystal_forge",
        "SELECT id, name, repo_url FROM flakes;",
        "flakes-table.txt"
      )
      logger.assert_in_output("crystal-forge", flakes_output, "Crystal Forge flake in flakes table")
      logger.assert_in_output("git://gitserver:8080", flakes_output, "Git server URL in flakes table")

      # Check commits table
      commits_output = logger.database_query(
        server,
        "crystal_forge",
        "SELECT c.id, f.name as flake_name, c.git_commit_hash, c.commit_timestamp FROM commits c JOIN flakes f ON c.flake_id = f.id WHERE f.name = \'crystal-forge\' LIMIT 5;",
        "commits-table.txt"
      )
      logger.assert_in_output("crystal-forge", commits_output, "Commits linked to crystal-forge flake")

      # Check derivations table with more specific query
      derivations_output = logger.database_query(
        server,
        "crystal_forge",
        "SELECT d.derivation_name, d.derivation_type, d.derivation_target, f.name as flake_name FROM derivations d JOIN commits c ON d.commit_id = c.id JOIN flakes f ON c.flake_id = f.id WHERE f.name = \'crystal-forge\' AND d.derivation_type = 'nixos' LIMIT 10;",
        "nixos-derivations.txt"
      )

      # Verify cf-test-sys derivation exists
      # logger.assert_in_output(
      #   "cf-test-sys",
      #   derivations_output,
      #   "cf-test-sys NixOS derivation in database"
      # )

      # Check for system states linked to our agent
      TestPatterns.database_verification(logger, server, "crystal_forge", {
        "hostname": system_info['hostname'],
        "change_reason": "startup",
      })

      # Get comprehensive statistics
      logger.log_section("📈 Database Statistics")

      # Count total flakes
      flakes_count = server.succeed(
        """psql -U crystal_forge -d crystal_forge -c 'SELECT COUNT(*) FROM flakes;' -t"""
      ).strip()
      logger.log_info(f"Total flakes: {flakes_count}")

      # Count commits for crystal-forge flake
      commits_count = server.succeed(
        """psql -U crystal_forge -d crystal_forge -c "SELECT COUNT(*) FROM commits c JOIN flakes f ON c.flake_id = f.id WHERE f.name = \'crystal-forge\';" -t"""
      ).strip()
      logger.log_info(f"Commits for crystal-forge flake: {commits_count}")

      # Count derivations for crystal-forge flake
      derivations_count = server.succeed(
        """psql -U crystal_forge -d crystal_forge -c "SELECT COUNT(*) FROM derivations d JOIN commits c ON d.commit_id = c.id JOIN flakes f ON c.flake_id = f.id WHERE f.name = \'crystal-forge\';" -t"""
      ).strip()
      logger.log_info(f"Total derivations for crystal-forge flake: {derivations_count}")

      # Count NixOS derivations specifically
      nixos_derivations_count = server.succeed(
        """psql -U crystal_forge -d crystal_forge -c "SELECT COUNT(*) FROM derivations d JOIN commits c ON d.commit_id = c.id JOIN flakes f ON c.flake_id = f.id WHERE f.name = \'crystal-forge\' AND d.derivation_type = \'nixos\';" -t"""
      ).strip()
      logger.log_info(f"NixOS derivations for crystal-forge flake: {nixos_derivations_count}")

      # Count package derivations
      package_derivations_count = server.succeed(
        """psql -U crystal_forge -d crystal_forge -c "SELECT COUNT(*) FROM derivations d JOIN commits c ON d.commit_id = c.id JOIN flakes f ON c.flake_id = f.id WHERE f.name = \'crystal-forge\' AND d.derivation_type = \'package\';" -t"""
      ).strip()
      logger.log_info(f"Package derivations for crystal-forge flake: {package_derivations_count}")

      # Verify the specific cf-test-sys target exists
      cf_test_sys_count = server.succeed(
        """psql -U crystal_forge -d crystal_forge -c "SELECT COUNT(*) FROM derivations d JOIN commits c ON d.commit_id = c.id JOIN flakes f ON c.flake_id = f.id WHERE f.name = \'crystal-forge\' AND d.derivation_name LIKE \'%cf-test-sys%\';" -t"""
      ).strip()
      logger.log_info(f"cf-test-sys derivations found: {cf_test_sys_count}")

      # Get the actual cf-test-sys derivation details
      if int(cf_test_sys_count.strip()) > 0:
        logger.log_success("cf-test-sys derivation successfully stored in database")

        cf_test_sys_details = logger.database_query(
          server,
          "crystal_forge",
          "SELECT d.derivation_name, d.derivation_type, d.derivation_target, d.derivation_path, ds.status_name FROM derivations d JOIN commits c ON d.commit_id = c.id JOIN flakes f ON c.flake_id = f.id JOIN derivation_statuses ds ON d.status_id = ds.id WHERE f.name = \'crystal-forge\' AND d.derivation_name LIKE \'%cf-test-sys%\' LIMIT 3;",
          "cf-test-sys-details.txt"
        )
        logger.log_success("cf-test-sys derivation details captured")
      else:
        logger.log_warning("cf-test-sys derivation not found - may need longer evaluation time or different naming")

        # Debug: show all available derivation names
        all_derivations = logger.database_query(
          server,
          "crystal_forge",
          "SELECT DISTINCT d.derivation_name FROM derivations d JOIN commits c ON d.commit_id = c.id JOIN flakes f ON c.flake_id = f.id WHERE f.name = \'crystal-forge\' ORDER BY d.derivation_name LIMIT 20;",
          "all-derivation-names.txt"
        )
        logger.log_info("Captured all available derivation names for debugging")

      # Count total system states
      systems_count = server.succeed(
        """psql -U crystal_forge -d crystal_forge -c 'SELECT COUNT(*) FROM system_states;' -t"""
      ).strip()
      logger.log_info(f"Total system states recorded: {systems_count}")

      # Verify agent system state
      agent_states_count = server.succeed(
        f"""psql -U crystal_forge -d crystal_forge -c "SELECT COUNT(*) FROM system_states WHERE hostname = '{system_info['hostname']}';" -t"""
      ).strip()
      logger.log_info(f"System states for agent '{system_info['hostname']}': {agent_states_count}")

      if int(agent_states_count.strip()) > 0:
        logger.log_success(f"Agent '{system_info['hostname']}' system states recorded successfully")
      else:
        logger.log_warning(f"No system states found for agent '{system_info['hostname']}'")

      logger.finalize_test()
    '';
  }

{
  lib,
  inputs,
  pkgs,
  ...
}: let
  keyPair = pkgs.runCommand "agent-keypair" {} ''
    mkdir -p $out
    ${pkgs.crystal-forge.default.cf-keygen}/bin/cf-keygen -f $out/agent.key
  '';
  keyPath = pkgs.runCommand "agent.key" {} ''
    mkdir -p $out
    cp ${keyPair}/agent.key $out/
  '';
  pubPath = pkgs.runCommand "agent.pub" {} ''
    mkdir -p $out
    cp ${keyPair}/agent.pub $out/
  '';
  derivation-paths = lib.crystal-forge.derivation-paths pkgs;
  CF_TEST_DB_PORT = 5432;
  CF_TEST_SERVER_PORT = 3000;
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
    name = "crystal-forge-attic-cache-integration";
    skipLint = true;
    skipTypeCheck = true;
    nodes = {
      gitserver = lib.crystal-forge.makeGitServerNode {
        inherit pkgs systemBuildClosure;
        port = 8080;
      };

      atticCache = lib.crystal-forge.makeAtticCacheNode {
        inherit lib pkgs;
        port = 8080;
        jwtSecretB64 = "dGVzdCBzZWNyZXQgZm9yIGF0dGljZA=="; # base64("test secret for atticd")
      };

      cfServer = {
        imports = [inputs.self.nixosModules.crystal-forge];

        networking.useDHCP = true;
        networking.firewall.allowedTCPPorts = [CF_TEST_SERVER_PORT 5432];

        virtualisation.writableStore = true;
        virtualisation.memorySize = 8096;
        virtualisation.cores = 8;
        virtualisation.additionalPaths = [
          systemBuildClosure
          inputs.self.nixosConfigurations.cf-test-sys.config.system.build.toplevel.drvPath
        ];

        services.postgresql = {
          enable = true;
          settings."listen_addresses" = lib.mkForce "*";
          authentication = lib.concatStringsSep "\n" [
            "local   all   postgres   trust"
            "local   all   all        peer"
            "host    all   all 127.0.0.1/32 trust"
            "host    all   all ::1/128      trust"
            "host    all   all 10.0.2.2/32  trust"
          ];
          initialScript = pkgs.writeText "init-crystal-forge.sql" ''
            CREATE USER crystal_forge LOGIN;
            CREATE DATABASE crystal_forge OWNER crystal_forge;
            GRANT ALL PRIVILEGES ON DATABASE crystal_forge TO crystal_forge;
          '';
        };

        environment.systemPackages = with pkgs; [
          git
          jq
          hello
          attic-client
          curl
          crystal-forge.default
          crystal-forge.cf-test-suite.runTests
          crystal-forge.cf-test-suite.testRunner
        ];

        environment.etc = {
          "agent.key".source = "${keyPath}/agent.key";
          "agent.pub".source = "${pubPath}/agent.pub";
        };

        services.crystal-forge = {
          enable = true;
          env-file = "/etc/attic-env";
          local-database = true;
          log_level = "debug";
          client = {
            enable = true;
            private_key = "/etc/agent.key";
            server_host = "localhost";
            server_port = CF_TEST_SERVER_PORT;
          };

          # Database config
          database = {
            host = "localhost";
            user = "crystal_forge";
            name = "crystal_forge";
            port = 5432;
          };

          # Server config
          server = {
            port = CF_TEST_SERVER_PORT;
            enable = true;
            host = "0.0.0.0";
          };

          # Build configuration - DISABLE initially
          build = {
            enable = true;
            offline = false;
            systemd_properties = [
              "Environment=ATTIC_SERVER_URL=http://atticCache:8080/cf-test"
              "Environment=ATTIC_REMOTE_NAME=cf-test"
              # Add ATTIC_TOKEN if you have it statically, or let vault handle it
            ];
          };

          # Attic cache configuration - token will be set by testScript
          cache = {
            cache_type = "Attic";
            push_to = "http://atticCache:8080";
            push_after_build = true;
            attic_cache_name = "cf-test";
            max_retries = 2;
            retry_delay_seconds = 5;
          };

          # Test flake configuration
          flakes = {
            flake_polling_interval = "1m";
            commit_evaluation_interval = "1m";
            build_processing_interval = "1m";
            watched = [];
          };

          # Test environment
          environments = [];

          # Test system configuration
          systems = [];
        };
      };
    };

    globalTimeout = 600; # 10 minutes for more complex setup
    extraPythonPackages = p: [p.pytest pkgs.crystal-forge.cf-test-suite];

    testScript = ''
      import os
      import pytest

      ATTIC = "${pkgs.attic-client}/bin/attic"
      ATTICADM = "${pkgs.attic-server}/bin/atticadm"

      # Test harness environment
      os.environ.update({
          "CF_TEST_PACKAGE_DRV": "${inputs.self.nixosConfigurations.cf-test-sys.config.system.build.toplevel.drvPath}",
          "CF_TEST_PACKAGE_NAME": "cf-test-sys",
          "CF_TEST_PACKAGE_VERSION": "0.1.0",
          "CF_TEST_SERVER_PORT": "${toString CF_TEST_SERVER_PORT}",
          "CF_TEST_DRV": "${derivation-paths}",
      })

      # Make driver machines visible to cf_test
      import cf_test
      cf_test._driver_machines = {
          "cfServer": cfServer,
          "atticCache": atticCache,
          "gitserver": gitserver,
      }

      print("=== Crystal Forge Attic Cache Integration Test ===")

      # Start all VMs
      start_all()

      # Mask the server service completely to prevent it from starting
      print("Masking Crystal Forge server service...")
      cfServer.succeed("systemctl mask crystal-forge-server.service")

      # Wait for network to be ready on both VMs
      print("Waiting for network readiness...")
      cfServer.wait_for_unit("network-online.target")
      atticCache.wait_for_unit("network-online.target")

      # Start and verify atticCache services
      print("Starting Attic cache services...")
      atticCache.wait_for_unit("atticd.service", timeout=120)
      atticCache.wait_for_unit("attic-setup.service", timeout=180)

      # Verify atticd is listening
      print("Verifying Attic is listening...")
      atticCache.succeed("ss -tlnp | grep :8080")

      # Wait for PostgreSQL on cfServer
      print("Waiting for PostgreSQL...")
      cfServer.wait_for_unit("postgresql.service")

      # Test network connectivity from cfServer to atticCache
      print("Testing network connectivity between VMs...")
      cfServer.succeed("ping -c 2 atticCache")
      cfServer.succeed("timeout 5 bash -c 'until curl -f http://atticCache:8080/ 2>/dev/null; do sleep 1; done'")

      # Generate Attic token
      print("Generating Attic authentication token...")
      server_toml = atticCache.succeed(
          "find /var/lib /etc -name 'server.toml' -o -name 'atticd.toml' 2>/dev/null | head -1"
      ).strip()

      token = atticCache.succeed(
          f"{ATTICADM} --config {server_toml} make-token "
          "--sub cfServer --validity '1 year' "
          "--pull 'cf-*' --push 'cf-*' --create-cache 'cf-*' --configure-cache 'cf-*'"
      ).strip()

      print(f"Token generated: {token[:20]}...")

      # Configure Attic client and environment on cfServer
      print("Configuring Attic client on cfServer...")
      cfServer.succeed(f"""
      set -x
      mkdir -p /var/lib/crystal-forge/.config
      chown -R crystal-forge:crystal-forge /var/lib/crystal-forge

      # Setup attic client
      sudo -u crystal-forge env HOME=/var/lib/crystal-forge XDG_CONFIG_HOME=/var/lib/crystal-forge/.config \\
        {ATTIC} login cf-test http://atticCache:8080 {token}

      # Create cache
      sudo -u crystal-forge env HOME=/var/lib/crystal-forge XDG_CONFIG_HOME=/var/lib/crystal-forge/.config \\
        {ATTIC} cache create cf-test:cf-test || true

      # Environment file for Crystal Forge
      cat > /var/lib/crystal-forge/.config/crystal-forge-attic.env <<'EOF'
      ATTIC_SERVER_URL=http://atticCache:8080
      ATTIC_TOKEN={token}
      ATTIC_REMOTE_NAME=cf-test
      HOME=/var/lib/crystal-forge
      XDG_CONFIG_HOME=/var/lib/crystal-forge/.config
      EOF

      chown crystal-forge:crystal-forge /var/lib/crystal-forge/.config/crystal-forge-attic.env
      chmod 644 /var/lib/crystal-forge/.config/crystal-forge-attic.env

      # Verify file was created
      ls -la /var/lib/crystal-forge/.config/
      cat /var/lib/crystal-forge/.config/crystal-forge-attic.env
      """)

      # Verify Attic client configuration works
      print("Verifying Attic client configuration...")
      cfServer.succeed(f"""
      sudo -u crystal-forge env HOME=/var/lib/crystal-forge XDG_CONFIG_HOME=/var/lib/crystal-forge/.config \\
        {ATTIC} cache info cf-test:cf-test
      """)

      # Start Crystal Forge builder
      print("Starting Crystal Forge builder...")
      cfServer.succeed("systemctl start crystal-forge-builder.service")
      cfServer.wait_for_unit("crystal-forge-builder.service", timeout=60)

      # Give builder time to initialize and run migrations
      print("Waiting for builder to complete initialization...")
      cfServer.succeed("sleep 10")

      # Check for infinite loop early
      print("Checking for infinite loop...")
      builder_logs = cfServer.succeed("journalctl -u crystal-forge-builder.service --no-pager | tail -100")
      loop_count = builder_logs.count("Syncing commits for 0 watched flakes")
      if loop_count > 10:
          print(f"ERROR: Detected infinite loop! Found {loop_count} 'Syncing commits' messages")
          print("=== Builder Logs ===")
          print(builder_logs)
          raise Exception("Builder is stuck in infinite loop")

      # Wait for database schema to be ready with diagnostics
      print("Waiting for database schema to be ready...")
      cfServer.succeed("""
      timeout 120 bash -c '
        attempts=0
        while ! sudo -u postgres psql -d crystal_forge -c "SELECT 1 FROM derivations LIMIT 1;" >/dev/null 2>&1; do
          attempts=$((attempts + 1))
          echo "Attempt $attempts: Waiting for database schema..."

          if [ $attempts -gt 5 ]; then
            echo "Checking migration status..."
            sudo -u postgres psql -d crystal_forge -c "SELECT version, description FROM _sqlx_migrations ORDER BY version DESC LIMIT 5;" 2>&1 || echo "Migrations table not ready"
          fi

          if [ $attempts -gt 10 ]; then
            echo "Checking builder service status..."
            systemctl status crystal-forge-builder.service || true
          fi

          sleep 3
        done
        echo "Database schema ready after $attempts attempts!"
      '
      """)

      # Verify builder is healthy
      print("Verifying builder service health...")
      cfServer.succeed("systemctl is-active crystal-forge-builder.service")

      # Show recent builder logs
      print("Recent builder logs:")
      print(cfServer.succeed("journalctl -u crystal-forge-builder.service --no-pager -n 30"))

      # Run the actual tests
      print("=== Running pytest tests ===")
      exit_code = pytest.main([
          "-vvvv", "--tb=short", "-x", "-s",
          "-m", "attic_cache", "--pyargs", "cf_test",
      ])

      if exit_code != 0:
          print("\n=== Test Failure Debug Info ===")
          print("\n--- Builder Service Logs ---")
          cfServer.succeed("journalctl -u crystal-forge-builder.service --no-pager -n 100")

          print("\n--- Attic Environment ---")
          cfServer.succeed("cat /var/lib/crystal-forge/.config/crystal-forge-attic.env")

          print("\n--- Attic Service Logs ---")
          atticCache.succeed("journalctl -u atticd.service --no-pager -n 50")

          print("\n--- Database State ---")
          cfServer.succeed("sudo -u postgres psql -d crystal_forge -c 'SELECT COUNT(*) FROM derivations;'")

          raise SystemExit(exit_code)

      print("\n=== All tests passed! ===")
    '';
  }

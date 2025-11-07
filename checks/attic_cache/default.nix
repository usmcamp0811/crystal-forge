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

          deployment = {
            cache_url = "http://atticCache:8080/cf-test";
            deployment_poll_interval = lib.mkForce "30";
            fallback_to_local_build = false;
          };
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
            # BUILD CONCURRENCY
            max_concurrent_derivations = 1; # 3 parallel builds
            max_jobs = 1; # 2 derivations per build
            cores_per_job = 4; # 4 cores per derivation
            # Math: 3 × 2 × 4 = 24 cores (leaves 8 for system/eval)

            # SYSTEMD LIMITS
            systemd_memory_max = "6G"; # 32GB per build (3 × 32G = 96GB, safe)
            systemd_cpu_quota = 400; # 8 cores per build scope (not per derivation!)
            use_systemd_scope = true;
            systemd_timeout_stop_sec = 900;

            # OTHER SETTINGS
            use_substitutes = true;
            poll_interval = "5s";
            sandbox = true;
            max_silent_time = "3h";
            timeout = "8h";
            systemd_properties = [
              "Environment=ATTIC_SERVER_URL=http://atticCache:8080/cf-test"
              "Environment=ATTIC_REMOTE_NAME=cf-test"
            ];
          };

          # Attic cache configuration - token will be set by testScript
          cache = {
            cache_type = "Attic";
            push_to = "http://atticCache:8080";
            attic_cache_name = "cf-test";
            push_after_build = true;
            parallel_uploads = 5;
            max_retries = 5;
            retry_delay_seconds = 5;
            poll_interval = "5s";
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

    globalTimeout = 300; # 10 minutes for more complex setup
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

      # Start services
      atticCache.wait_for_unit("atticd.service")
      atticCache.wait_for_unit("attic-setup.service")
      cfServer.wait_for_unit("postgresql.service")
      cfServer.succeed("systemctl stop crystal-forge-server.service")

      # Generate Attic token
      server_toml = atticCache.succeed(
          "find /var/lib /etc -name 'server.toml' -o -name 'atticd.toml' 2>/dev/null | head -1"
      ).strip()

      token = atticCache.succeed(
          f"{ATTICADM} --config {server_toml} make-token "
          "--sub cfServer --validity '1 year' "
          "--pull 'cf-*' --push 'cf-*' --create-cache 'cf-*' --configure-cache 'cf-*'"
      ).strip()

      # Configure Attic client and environment
      cfServer.succeed(f"""
      mkdir -p /var/lib/crystal-forge/.config
      chown -R crystal-forge:crystal-forge /var/lib/crystal-forge

      # Setup attic client
      sudo -u crystal-forge env HOME=/var/lib/crystal-forge XDG_CONFIG_HOME=/var/lib/crystal-forge/.config \\
        {ATTIC} login cf-test http://atticCache:8080 {token}

      # Create cache
      sudo -u crystal-forge env HOME=/var/lib/crystal-forge XDG_CONFIG_HOME=/var/lib/crystal-forge/.config \\
        {ATTIC} cache create cf-test:cf-test || true

      # Environment for Crystal Forge (must match module location)
      cat > /var/lib/crystal-forge/.config/crystal-forge-attic.env <<EOF
      ATTIC_SERVER_URL=http://atticCache:8080
      ATTIC_TOKEN={token}
      ATTIC_REMOTE_NAME=cf-test
      HOME=/var/lib/crystal-forge
      XDG_CONFIG_HOME=/var/lib/crystal-forge/.config
      EOF
      chmod 644 /var/lib/crystal-forge/.config/crystal-forge-attic.env
      chown crystal-forge:crystal-forge /var/lib/crystal-forge/.config/crystal-forge-attic.env
      """)

      # Ensure runtime directory exists for builder
      cfServer.succeed("mkdir -p /run/crystal-forge")
      cfServer.succeed("chmod 755 /run/crystal-forge")
      cfServer.succeed("chown crystal-forge:crystal-forge /run/crystal-forge")

      # Start Crystal Forge builder
      cfServer.succeed("systemctl start crystal-forge-builder.service")
      cfServer.wait_for_unit("crystal-forge-builder.service")
      cfServer.succeed("sleep 10")  # Give builder time to read env and attempt auth

      # Wait for database schema to be ready
      cfServer.succeed("""
      timeout 60 bash -c '
        while ! sudo -u postgres psql -d crystal_forge -c "SELECT 1 FROM derivations LIMIT 1;" >/dev/null 2>&1; do
          echo "Waiting for database schema..."
          sleep 2
        done
        echo "Database schema ready!"
      '
      """)

      # Give builder and server time to stabilize
      cfServer.succeed("sleep 15")

      # Diagnostic check before running tests
      print("=== Pre-test diagnostics ===")
      cfServer.succeed("systemctl status crystal-forge-builder.service || true")
      cfServer.succeed("systemctl status crystal-forge-server.service || true")
      cfServer.succeed("ls -la /var/lib/crystal-forge/.config/ || true")
      cfServer.succeed("ps aux | grep -i crystal-forge | head -5 || true")

      # Stop server service - not needed for cache push tests and it pollutes logs
      print("=== Stopping server service (not needed for cache tests) ===")
      cfServer.succeed("systemctl stop crystal-forge-server.service || true")
      cfServer.succeed("sleep 2")

      # Run tests
      exit_code = pytest.main([
          "-vvvv", "--tb=short", "-x", "-s",
          "-m", "attic_cache", "--pyargs", "cf_test",
      ])

      if exit_code != 0:
          print("=== Test Failure Debug Info ===")
          cfServer.succeed("journalctl -u crystal-forge-builder.service --no-pager -n 50 || true")
          cfServer.succeed("cat /etc/attic-env || true")
          raise SystemExit(exit_code)

      print("All tests passed!")
    '';
  }

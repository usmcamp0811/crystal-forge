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
          "local-database" = true;
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
            enable = false;
            host = "0.0.0.0";
          };

          # Build configuration - DISABLE initially
          build = {
            enable = false; # Will be enabled after token setup
            offline = false;
            systemd_properties = [
              "EnvironmentFile=-/etc/attic-env"
              "Environment=HOME=/var/lib/crystal-forge"
              "Environment=XDG_CONFIG_HOME=/var/lib/crystal-forge/.config"
              "Environment=NIX_LOG=trace"
              "Environment=NIX_SHOW_STATS=1"
            ];
          };

          # Attic cache configuration - token will be set by testScript
          cache = {
            cache_type = "Attic";
            push_after_build = true;
            attic_cache_name = "cf-test";
            max_retries = 2;
            retry_delay_seconds = 1;
          };

          # Test flake configuration
          flakes = {
            flake_polling_interval = "1m";
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
      import time

      ATTIC = "${pkgs.attic-client}/bin/attic"
      ATTICADM = "${pkgs.attic-server}/bin/atticadm"
      ATTIC_SECRET_B64 = "dGVzdCBzZWNyZXQgZm9yIGF0dGljZA=="

      # Test harness env
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

      print("=== Starting Crystal Forge Attic Cache Integration Test ===")

      # -------- 1) Start Attic Cache --------
      print("Starting Attic cache services...")
      atticCache.wait_for_unit("atticd.service")
      atticCache.wait_for_unit("attic-setup.service")
      print("✅ Attic cache is ready")

      # -------- 2) Start cfServer database (but NOT crystal-forge yet) --------
      print("Starting cfServer database...")
      cfServer.wait_for_unit("postgresql.service")
      print("✅ Database is ready")

      # -------- 3) Generate Attic token with push permissions --------
      print("Generating Attic token with push permissions...")

      server_toml = atticCache.succeed(
          "set -e; "
          "for p in "
          "/var/lib/attic/server.toml "
          "/var/lib/atticd/server.toml "
          "/etc/atticd.toml "
          "/etc/attic/server.toml "
          "; do [ -f \"$p\" ] && { echo \"$p\"; exit 0; }; done; "
          "exit 1"
      ).strip()

      print(f"Using server config: {server_toml}")

      token = atticCache.succeed(
          f"{ATTICADM} --config {server_toml} make-token "
          "--sub cfServer "
          "--validity '1 year' "
          "--pull 'cf-*' "
          "--push 'cf-*' "
          "--create-cache 'cf-*' "
          "--configure-cache 'cf-*'"
      ).strip()

      # Validate token format
      parts = token.split(".")
      if len(parts) != 3 or not all(parts):
          raise AssertionError(f"Invalid JWT token format: {token[:40]}...")

      print(f"✅ Generated token: {token[:32]}...")

      # -------- 4) Configure cfServer with Attic credentials --------
      print("Configuring cfServer with Attic credentials...")
      cfServer.succeed(f"""
      set -euo pipefail

      # Ensure crystal-forge user and directories exist
      mkdir -p /var/lib/crystal-forge/.config
      chown -R crystal-forge:crystal-forge /var/lib/crystal-forge || {{
        # If user doesn't exist yet, create it
        useradd -r -s /bin/sh -d /var/lib/crystal-forge crystal-forge || true
        chown -R crystal-forge:crystal-forge /var/lib/crystal-forge
      }}
      chmod 755 /var/lib/crystal-forge/.config

      # Use attic login command to properly configure the client
      sudo -u crystal-forge env HOME=/var/lib/crystal-forge XDG_CONFIG_HOME=/var/lib/crystal-forge/.config \\
        {ATTIC} login local http://atticCache:8080 {token}

      # Write environment file for systemd services
      cat > /etc/attic-env <<EOF
      ATTIC_SERVER_URL=http://atticCache:8080
      ATTIC_TOKEN={token}
      ATTIC_REMOTE_NAME=local
      HOME=/var/lib/crystal-forge
      XDG_CONFIG_HOME=/var/lib/crystal-forge/.config
      EOF
      chmod 644 /etc/attic-env
      """)

      print("✅ cfServer configured with Attic credentials")

      # -------- 5) Test Attic token functionality --------
      print("Testing Attic token functionality...")

      # Test creating the cache
      cfServer.succeed(
          "sudo -u crystal-forge env HOME=/var/lib/crystal-forge XDG_CONFIG_HOME=/var/lib/crystal-forge/.config " +
          f"{ATTIC} cache create local:cf-test || true"
      )

      # Test cache info (verifies read access)
      cfServer.succeed(
          "sudo -u crystal-forge env HOME=/var/lib/crystal-forge XDG_CONFIG_HOME=/var/lib/crystal-forge/.config " +
          f"{ATTIC} cache info local:cf-test"
      )

      # Test push functionality with hello package
      hello_store_path = cfServer.succeed(
          "readlink -f $(which hello) | sed 's#/bin/hello##'"
      ).strip()

      if not hello_store_path.startswith("/nix/store/"):
          raise AssertionError(f"Unexpected hello store path: {hello_store_path}")

      print(f"Testing push with: {hello_store_path}")
      cfServer.succeed(
          "sudo -u crystal-forge env HOME=/var/lib/crystal-forge XDG_CONFIG_HOME=/var/lib/crystal-forge/.config " +
          f"{ATTIC} push local:cf-test {hello_store_path}"
      )

      # Verify the push worked
      hello_basename = cfServer.succeed(f"basename '{hello_store_path}'").strip()
      cfServer.succeed(
          "sudo -u crystal-forge env HOME=/var/lib/crystal-forge XDG_CONFIG_HOME=/var/lib/crystal-forge/.config " +
          f"{ATTIC} ls local:cf-test | grep -F {hello_basename}"
      )

      print("✅ Attic token test passed!")

      # -------- 6) Enable Crystal Forge builder service --------
      print("Enabling Crystal Forge builder service...")

      # Update the Crystal Forge configuration to enable the builder
      cfServer.succeed("""
      # Create a temporary config file to enable the builder
      mkdir -p /tmp/cf-config
      cat > /tmp/cf-config/override.nix <<'EOF'
      {
        services.crystal-forge.build.enable = true;
      }
      EOF

      # Apply the configuration change
      nixos-rebuild switch --flake /etc/nixos#cfServer --override-input crystal-forge /tmp/cf-config || {
        # Fallback: directly enable the systemd service
        systemctl enable crystal-forge-builder.service
        systemctl start crystal-forge-builder.service
      }
      """)

      # Wait for the builder service to start
      cfServer.wait_for_unit("crystal-forge-builder.service")
      print("✅ Crystal Forge builder service is running")

      # -------- 7) Test Crystal Forge integration --------
      print("Testing Crystal Forge integration with Attic...")

      # Wait a bit for the service to fully initialize
      time.sleep(5)

      # Verify the service can connect to Attic
      cfServer.succeed(
          "systemctl status crystal-forge-builder.service"
      )

      # Check logs for any immediate errors
      logs = cfServer.succeed(
          "journalctl -u crystal-forge-builder.service --no-pager -n 20"
      )
      print(f"Builder service logs:\n{logs}")

      print("✅ Crystal Forge is ready for testing")

      # -------- 8) Run pytest suite --------
      print("Running pytest attic_cache tests...")
      exit_code = pytest.main([
          "-vvvv", "--tb=short", "-x", "-s",
          "-m", "attic_cache", "--pyargs", "cf_test",
      ])

      if exit_code != 0:
          # Print additional debugging info on failure
          print("=== Test failure debugging info ===")

          print("Crystal Forge builder service status:")
          cfServer.succeed("systemctl status crystal-forge-builder.service || true")

          print("\nCrystal Forge builder service logs:")
          cfServer.succeed("journalctl -u crystal-forge-builder.service --no-pager -n 50 || true")

          print("\nAttic cache status:")
          atticCache.succeed("systemctl status atticd.service || true")

          print("\nAttic cache logs:")
          atticCache.succeed("journalctl -u atticd.service --no-pager -n 30 || true")

          print("\nAttic client config:")
          cfServer.succeed("sudo -u crystal-forge cat /var/lib/crystal-forge/.config/attic/config.toml || true")

          print("\nEnvironment file:")
          cfServer.succeed("cat /etc/attic-env || true")

          raise SystemExit(exit_code)

      print("✅ All tests passed!")
    '';
  }

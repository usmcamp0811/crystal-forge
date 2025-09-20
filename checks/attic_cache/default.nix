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
        systemd.services.crystal-forge-builder = {
          after = ["attic-client-setup.service"];
          wants = ["attic-client-setup.service"];
        };
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

        # Add attic client configuration for the cfServer
        systemd.services.attic-client-setup = {
          description = "Setup Attic client for Crystal Forge";
          wants = ["network-online.target"];
          after = ["network-online.target" "attic-setup.service"];
          before = ["crystal-forge-builder.service"];
          wantedBy = ["multi-user.target"];

          environment = {
            HOME = "/root";
            # + openssl for HMAC signing
            PATH = lib.mkForce "${pkgs.attic-server}/bin:${pkgs.attic-client}/bin:${pkgs.openssl}/bin:${pkgs.curl}/bin:${pkgs.iputils}/bin:${pkgs.dnsutils}/bin:${pkgs.netcat}/bin:${pkgs.coreutils}/bin";
          };

          script = ''
            set -euo pipefail
            echo "Setting up Attic environment for Crystal Forge..."

            # Wait for atticCache HTTP to be reachable
            for i in {1..60}; do
              if curl -sf http://atticCache:8080/ >/dev/null 2>&1; then
                echo "Attic server is ready after $i attempts"
                break
              fi
              if [ "$i" -eq 60 ]; then
                echo "ERROR: Attic server failed to become available after 60 attempts"
                exit 1
              fi
              echo "Waiting for attic server... attempt $i/60"
              sleep 3
            done

            # === Mint HS256 JWT in pure shell ===
            # This must match token-hs256-secret-base64 on the server.
            SECRET_B64="dGVzdCBzZWNyZXQgZm9yIGF0dGljZA=="

            b64url() {
              # stdin -> base64url (no padding)
              base64 -w0 | tr '+/' '-_' | tr -d '='
            }

            now=$(date +%s)
            exp=$(( now + 1800 ))  # 30 minutes

            header='{"alg":"HS256","typ":"JWT"}'
            payload='{"sub":"cfServer","exp":'"$exp"'}'

            header_b64=$(printf '%s' "$header"  | b64url)
            payload_b64=$(printf '%s' "$payload" | b64url)
            signing_input="$header_b64.$payload_b64"

            # HMAC-SHA256(signing_input, secret)
            signature=$(printf '%s' "$signing_input" \
              | openssl dgst -sha256 -mac HMAC -macopt "key:$(printf %s "$SECRET_B64" | base64 -d)" -binary \
              | b64url)

            TOKEN="$signing_input.$signature"
            echo "Minted JWT valid until $exp"

            # Create shared attic config directory
            echo "Logging in to Attic server with minted token..."
            attic login local http://atticCache:8080 "$TOKEN"

            echo "Verifying cache exists (local:test)..."
            attic cache info local:test || echo "Cache verification failed but continuing"

            # Export environment for the builder service
            cat >/etc/attic-env <<EOF
            ATTIC_SERVER_URL=http://atticCache:8080
            ATTIC_TOKEN=$TOKEN
            ATTIC_REMOTE_NAME=local
            EOF
            chmod 0640 /etc/attic-env

            # Restart builder to pick up new environment
            systemctl daemon-reload || true
            systemctl try-restart crystal-forge-builder.service || true

            echo "Attic client setup completed successfully"
          '';

          serviceConfig = {
            Type = "oneshot";
            RemainAfterExit = true;
            User = "root";
            Group = "root";
          };
        };

        services.crystal-forge = {
          enable = true;
          "local-database" = true;
          log_level = "debug";
          client.enable = false;

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

          # Build configuration with Attic environment variables
          build = {
            enable = true;
            offline = false;
            systemd_properties = [
              # Ensure the environment file is loaded
              "EnvironmentFile=-/etc/attic-env"
              # Also set them directly as backup
              "Environment=ATTIC_SERVER_URL=http://atticCache:8080"
              "Environment=ATTIC_REMOTE_NAME=local"
              "Environment=HOME=/root"
              "Environment=NIX_LOG=trace"
              "Environment=NIX_SHOW_STATS=1"
            ];
          };

          # Attic cache configuration
          cache = {
            cache_type = "Attic";
            push_after_build = true;
            attic_cache_name = "test";
            # Remove this line: attic_token = "dGVzdCBzZWNyZXQgZm9yIGF0dGljZA==";
            max_retries = 2;
            retry_delay_seconds = 1;
          };

          # Test flake configuration - this is what the test expects
          flakes = {
            flake_polling_interval = "1m";
            watched = [
              # {
              #   name = "test-flake";
              #   repo_url = "http://gitserver/crystal-forge";
              #   auto_poll = true;
              #   initial_commit_depth = 5;
              # }
            ];
          };

          # Test environment
          environments = [
            # {
            #   name = "test";
            #   description = "Test environment for Crystal Forge agents and evaluation";
            #   is_active = true;
            #   risk_profile = "LOW";
            #   compliance_level = "NONE";
            # }
          ];

          # Test system configuration
          systems = [
            # {
            #   hostname = "agent";
            #   public_key = lib.strings.trim (builtins.readFile "${pubPath}/agent.pub");
            #   environment = "test";
            #   flake_name = "test-flake";
            # }
          ];
        };
      };
    };

    globalTimeout = 300; # 5 minutes
    extraPythonPackages = p: [p.pytest pkgs.crystal-forge.cf-test-suite];

    testScript = ''
      import os
      import pytest

      # Set test-specific environment variables
      os.environ.update({
          "CF_TEST_PACKAGE_DRV": "${inputs.self.nixosConfigurations.cf-test-sys.config.system.build.toplevel.drvPath}",
          "CF_TEST_PACKAGE_NAME": "cf-test-sys",
          "CF_TEST_PACKAGE_VERSION": "0.1.0",
          "CF_TEST_SERVER_PORT": "${toString CF_TEST_SERVER_PORT}",
          "CF_TEST_DRV": "${derivation-paths}",
      })

      # Configure machine access for cf_test
      import cf_test
      cf_test._driver_machines = {
          "cfServer": cfServer,
          "atticCache": atticCache,
          "gitserver": gitserver,
      }

      atticCache.wait_for_unit("atticd.service")
      # Wait for attic client setup to complete
      # In your test script, ensure server setup completes first
      atticCache.wait_for_unit("attic-setup.service")

      # Then start the client setup
      cfServer.wait_for_unit("attic-client-setup.service")

      # Verify Crystal Forge builder is running
      cfServer.wait_for_unit("crystal-forge-builder.service")

      atticCache.succeed("${pkgs.attic-client}/bin/attic cache info local:test")

      # Run the attic cache tests
      exit_code = pytest.main([
          "-vvvv", "--tb=short", "-x", "-s",
          "-m", "attic_cache", "--pyargs", "cf_test",
      ])

      if exit_code != 0:
          raise SystemExit(exit_code)
    '';
  }

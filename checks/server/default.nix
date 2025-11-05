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
    name = "crystal-forge-server-integration-test";
    skipLint = true;
    skipTypeCheck = true;
    nodes = {
      gitserver = lib.crystal-forge.makeGitServerNode {
        inherit pkgs systemBuildClosure;
        port = 8080;
      };

      server = {
        nix.settings = {
          experimental-features = ["nix-command" "flakes"];
          # use-registries = false;
        };
        imports = [inputs.self.nixosModules.crystal-forge];

        networking.useDHCP = true;
        networking.firewall.allowedTCPPorts = [CF_TEST_SERVER_PORT 5432];

        virtualisation.writableStore = true;
        virtualisation.memorySize = 6144; # Increased from 4GB to 6GB
        virtualisation.cores = 4;
        virtualisation.diskSize = 16384;
        virtualisation.additionalPaths =
          [
            systemBuildClosure
            inputs.self.nixosConfigurations.cf-test-sys.config.system.build.toplevel.drvPath
            # Add any flake inputs that might be referenced
            inputs.nixpkgs.outPath
          ]
          ++ lib.crystal-forge.prefetchedPaths;
        systemd.tmpfiles.rules = [
          "d /var/lib/crystal-forge 0755 crystal-forge crystal-forge -"
          "d /var/lib/crystal-forge/.cache 0755 crystal-forge crystal-forge -"
          "d /var/lib/crystal-forge/.cache/nix 0755 crystal-forge crystal-forge -"
          "Z /var/lib/crystal-forge/.cache/nix - crystal-forge crystal-forge -"
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
          openssl
          curl
          crystal-forge.default
          crystal-forge.cf-test-suite.runTests
          crystal-forge.cf-test-suite.testRunner
        ];

        # Set system-wide environment variables for Nix evaluation
        environment.variables = {
          TMPDIR = "/tmp";
          TMP = "/tmp";
          TEMP = "/tmp";
        };

        environment.etc = {
          "server.key".source = "${keyPath}/agent.key";
          "server.pub".source = "${pubPath}/agent.pub";
        };

        services.crystal-forge = {
          enable = true;
          local-database = true;
          log_level = "debug";
          client = {
            enable = true;
            server_host = "localhost";
            server_port = 3000;
            private_key = "/etc/server.key";
          };

          # Database config
          database = {
            host = "localhost";
            user = "crystal_forge";
            name = "crystal_forge";
            port = 5432;
          };

          # Server config - ENABLE this for dry-run tests
          server = {
            enable = true;
            port = CF_TEST_SERVER_PORT;
            host = "0.0.0.0";
          };

          # Build configuration - DISABLED for server-only dry-run tests
          build = {
            enable = false;
            offline = true;
          };

          # Test flake configuration
          flakes = {
            flake_polling_interval = "1m";
            watched = [
              {
                name = "test-flake";
                repo_url = "http://gitserver/crystal-forge";
                auto_poll = true;
                initial_commit_depth = 5;
              }
            ];
          };

          # Test environment
          environments = [
            {
              name = "test";
              description = "Test environment for Crystal Forge dry-run evaluation";
              is_active = true;
              risk_profile = "LOW";
              compliance_level = "NONE";
            }
          ];

          # Test system configuration
          systems = [
            {
              hostname = "server";
              public_key = lib.strings.trim (builtins.readFile "${pubPath}/agent.pub");
              environment = "test";
              flake_name = "test-flake";
            }
          ];
        };
      };
    };

    globalTimeout = 600; # 10 minutes for dry-run operations
    extraPythonPackages = p: [p.pytest pkgs.crystal-forge.cf-test-suite];

    testScript = ''
      import os
      import pytest

      os.environ["NIXOS_TEST_DRIVER"] = "1"
      start_all()

      # Wait for PostgreSQL
      server.wait_for_unit("postgresql.service")
      server.wait_for_unit("crystal-forge-server.service")
      server.wait_for_open_port(5432)
      server.wait_for_open_port(${toString CF_TEST_SERVER_PORT})

      # Forward ports for test access
      server.forward_port(5433, 5432)
      server.forward_port(${toString CF_TEST_SERVER_PORT}, ${toString CF_TEST_SERVER_PORT})

      from cf_test.vm_helpers import wait_for_git_server_ready
      wait_for_git_server_ready(gitserver, timeout=120)

      # Read commit hashes directly from testFlake metadata files
      main_head = "${lib.strings.trim (builtins.readFile (lib.crystal-forge.testFlake + "/MAIN_HEAD"))}"
      dev_head = "${lib.strings.trim (builtins.readFile (lib.crystal-forge.testFlake + "/DEVELOPMENT_HEAD"))}"
      feature_head = "${lib.strings.trim (builtins.readFile (lib.crystal-forge.testFlake + "/FEATURE_HEAD"))}"

      # Multi-branch commit hashes (read from testFlake metadata)
      main_commits = """${lib.strings.trim (builtins.readFile (lib.crystal-forge.testFlake + "/MAIN_COMMITS"))}"""
      dev_commits = """${lib.strings.trim (builtins.readFile (lib.crystal-forge.testFlake + "/DEVELOPMENT_COMMITS"))}"""
      feature_commits = """${lib.strings.trim (builtins.readFile (lib.crystal-forge.testFlake + "/FEATURE_COMMITS"))}"""

      # Set up test environment variables for multi-branch test flake
      os.environ["CF_TEST_GIT_SERVER_URL"] = "http://gitserver/crystal-forge"
      os.environ["CF_TEST_REAL_REPO_URL"] = "http://gitserver/crystal-forge"

      # Use main branch head as the primary test commit
      os.environ["CF_TEST_REAL_COMMIT_HASH"] = main_head

      # Branch head commits
      os.environ["CF_TEST_MAIN_HEAD"] = main_head
      os.environ["CF_TEST_DEVELOPMENT_HEAD"] = dev_head
      os.environ["CF_TEST_FEATURE_HEAD"] = feature_head

      # Commit lists (convert newlines to commas for easier parsing)
      os.environ["CF_TEST_MAIN_COMMITS"] = main_commits.replace('\n', ',')
      os.environ["CF_TEST_DEVELOPMENT_COMMITS"] = dev_commits.replace('\n', ',')
      os.environ["CF_TEST_FEATURE_COMMITS"] = feature_commits.replace('\n', ',')

      # Commit counts
      os.environ["CF_TEST_MAIN_COMMIT_COUNT"] = "${lib.strings.trim (builtins.readFile (lib.crystal-forge.testFlake + "/MAIN_COMMIT_COUNT"))}"
      os.environ["CF_TEST_DEVELOPMENT_COMMIT_COUNT"] = "${lib.strings.trim (builtins.readFile (lib.crystal-forge.testFlake + "/DEVELOPMENT_COMMIT_COUNT"))}"
      os.environ["CF_TEST_FEATURE_COMMIT_COUNT"] = "${lib.strings.trim (builtins.readFile (lib.crystal-forge.testFlake + "/FEATURE_COMMIT_COUNT"))}"

      # Database connection info
      os.environ["CF_TEST_DB_HOST"] = "127.0.0.1"
      os.environ["CF_TEST_DB_PORT"] = "5433"
      os.environ["CF_TEST_DB_USER"] = "postgres"
      os.environ["CF_TEST_DB_PASSWORD"] = ""  # no password for VM postgres

      # Server connection info
      os.environ["CF_TEST_SERVER_HOST"] = "127.0.0.1"
      os.environ["CF_TEST_SERVER_PORT"] = "${toString CF_TEST_SERVER_PORT}"

      # Derivation paths JSON
      os.environ["CF_TEST_DRV"] = "${derivation-paths}"

      # Flake information for tests
      os.environ["CF_TEST_FLAKE_NAME"] = "test-flake"
      os.environ["CF_TEST_PRELOADED_FLAKE_PATH"] = "/etc/preloaded-flake"

      # Inject machines for test access
      import cf_test
      cf_test._driver_machines = {
          "server": server,
          "gitserver": gitserver,
      }

      # Run dry-run specific tests

      # exit_code = pytest.main([
      #     "-vvvv",          # very verbose
      #     "--tb=short",     # concise tracebacks
      #     "-s",             # allow print() output
      #     "-ra",            # show summary of all failures, skips, etc.
      #     "-m", "server",   # run tests marked 'server'
      #     "--pyargs", "cf_test",
      # ])
      exit_code = pytest.main([
          "-vvvv",
          "--tb=short",
          "-x",
          "-s",
          "-m", "server",
          "--pyargs", "cf_test",
      ])
      if exit_code != 0:
          raise SystemExit(exit_code)
    '';
  }

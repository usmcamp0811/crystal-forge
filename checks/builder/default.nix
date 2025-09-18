{
  lib,
  inputs,
  pkgs,
  ...
}: let
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
    name = "crystal-forge-builder-test";
    skipLint = true;
    skipTypeCheck = true;
    nodes = {
      gitserver = lib.crystal-forge.makeGitServerNode {
        inherit pkgs systemBuildClosure;
        port = 8080;
      };

      cfServer = {
        imports = [inputs.self.nixosModules.crystal-forge];

        networking.useDHCP = true;
        networking.firewall.allowedTCPPorts = [5432];

        virtualisation.writableStore = true;
        virtualisation.memorySize = 4096;
        virtualisation.cores = 4;
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
          curl
          crystal-forge.default
          crystal-forge.cf-test-modules.runTests
          crystal-forge.cf-test-modules.testRunner
        ];

        services.crystal-forge = {
          enable = true;
          local-database = true;
          log_level = "debug";
          client.enable = false;

          # Database config
          database = {
            host = "localhost";
            user = "crystal_forge";
            name = "crystal_forge";
            port = 5432;
          };

          # Server disabled for builder-only tests
          server.enable = false;

          # Build configuration
          build = {
            enable = true;
            offline = false;
          };

          # Minimal cache configuration to prevent service crashes
          cache = {
            # cache_type = "None";
            push_after_build = false;
            push_to = null;
          };
        };
      };
    };

    globalTimeout = 300; # 5 minutes for builder tests
    extraPythonPackages = p: [p.pytest pkgs.crystal-forge.vm-test-logger pkgs.crystal-forge.cf-test-modules];

    testScript = ''
      import os
      import pytest

      # Set test environment variables
      os.environ.update({
          "CF_TEST_DB_HOST": "127.0.0.1",
          "CF_TEST_DB_PORT": "5433",
          "CF_TEST_DB_USER": "postgres",
          "CF_TEST_DB_PASSWORD": "",
          "CF_TEST_SERVER_HOST": "127.0.0.1",
          "CF_TEST_SERVER_PORT": "${toString CF_TEST_SERVER_PORT}",
          "CF_TEST_DRV": "${derivation-paths}",
          "CF_TEST_GIT_SERVER_URL": "http://gitserver/crystal-forge",
          "CF_TEST_REAL_COMMIT_HASH": "${lib.strings.trim (builtins.readFile (lib.crystal-forge.testFlake + "/MAIN_HEAD"))}",
      })

      # Configure machine access for cf_test
      import cf_test
      cf_test._driver_machines = {
          "cfServer": cfServer,
          "gitserver": gitserver,
      }

      # Start and wait for services
      start_all()

      cfServer.wait_for_unit("postgresql.service")
      cfServer.wait_for_unit("crystal-forge-builder.service")
      cfServer.wait_for_open_port(5432)
      cfServer.forward_port(5433, 5432)

      from cf_test.vm_helpers import wait_for_git_server_ready
      wait_for_git_server_ready(gitserver, timeout=60)

      # Run builder tests
      exit_code = pytest.main([
          "-vvvv", "--tb=short", "-x", "-s",
          "-m", "builder", "--pyargs", "cf_test",
      ])
      if exit_code != 0:
          raise SystemExit(exit_code)
    '';
  }

{
  lib,
  inputs,
  pkgs,
  ...
}: let
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
    name = "crystal-forge-grafana-dashboard";
    skipLint = true;
    skipTypeCheck = true;

    nodes = {
      gitserver = lib.crystal-forge.makeGitServerNode {
        inherit pkgs systemBuildClosure;
        port = 8080;
      };

      server = {
        imports = [inputs.self.nixosModules.crystal-forge];

        networking.useDHCP = true;
        networking.firewall.allowedTCPPorts = [5432 3000];

        virtualisation.writableStore = true;
        virtualisation.memorySize = 2048;
        virtualisation.cores = 2;
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
            CREATE USER grafana LOGIN;
            CREATE DATABASE crystal_forge OWNER crystal_forge;
            GRANT ALL PRIVILEGES ON DATABASE crystal_forge TO crystal_forge;
            GRANT CONNECT ON DATABASE crystal_forge TO grafana;
            GRANT USAGE ON SCHEMA public TO grafana;
            GRANT SELECT ON ALL TABLES IN SCHEMA public TO grafana;
            ALTER DEFAULT PRIVILEGES IN SCHEMA public GRANT SELECT ON TABLES TO grafana;
          '';
        };

        environment.systemPackages = with pkgs; [
          git
          jq
          hello
          curl
          crystal-forge.default
          crystal-forge.cf-test-suite.runTests
          crystal-forge.cf-test-suite.testRunner
        ];

        services.crystal-forge = {
          enable = true;
          local-database = true;
          log_level = "info";

          # Disable components not needed for dashboard testing
          server.enable = true;
          server.host = "127.0.0.1";
          server.port = 8000;
          build.enable = false;
          client.enable = false;

          # Database configuration
          database = {
            host = "localhost";
            user = "crystal_forge";
            name = "crystal_forge";
            port = 5432;
          };

          # Enable Grafana dashboard
          dashboards = {
            enable = true;
            datasource = {
              name = "Crystal Forge PostgreSQL";
              host = "127.0.0.1";
              port = 5432;
              database = "crystal_forge";
              user = "grafana";
              sslMode = "disable";
            };
            grafana = {
              provision = true;
              disableDeletion = true;
            };
          };

          # Minimal cache configuration
          cache = {
            push_after_build = false;
            push_to = null;
          };
        };

        # Grafana configuration
        services.grafana = {
          enable = true;
          settings = {
            server = {
              http_addr = "127.0.0.1";
              http_port = 3000;
            };
            security = {
              admin_user = "admin";
              admin_password = "admin";
            };
          };
        };
      };
    };

    globalTimeout = 600; # 10 minutes for dashboard + grafana startup
    extraPythonPackages = p: [p.pytest p.requests pkgs.crystal-forge.cf-test-suite];

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
          "CF_TEST_SERVER_PORT": "8000",
          "CF_TEST_GIT_SERVER_URL": "http://gitserver/crystal-forge",
          "CF_TEST_REAL_COMMIT_HASH": "${lib.strings.trim (builtins.readFile (lib.crystal-forge.testFlake + "/MAIN_HEAD"))}",
      })

      # Configure machine access for cf_test
      import cf_test
      cf_test._driver_machines = {
          "server": server,
          "gitserver": gitserver,
      }

      # Start all machines
      start_all()

      # Wait for PostgreSQL
      server.wait_for_unit("postgresql.service")
      server.wait_for_open_port(5432)
      server.forward_port(5433, 5432)

      # Wait for Crystal Forge server
      server.wait_for_unit("crystal-forge-server.service")
      server.wait_for_open_port(8000)

      # Wait for Grafana
      server.wait_for_unit("grafana.service")
      server.wait_for_open_port(3000)

      # Wait for git server
      from cf_test.vm_helpers import wait_for_git_server_ready
      wait_for_git_server_ready(gitserver, timeout=60)

      # Run Grafana dashboard tests
      exit_code = pytest.main([
          "-vvvv", "--tb=short", "-x", "-s",
          "-m", "dashboard",
          "--pyargs", "cf_test",
      ])
      if exit_code != 0:
          raise SystemExit(exit_code)
    '';
  }

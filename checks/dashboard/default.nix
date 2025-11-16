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
  CF_TEST_SERVER_PORT = 8000;

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
      server = {
        imports = [inputs.self.nixosModules.crystal-forge];

        networking.useDHCP = true;
        # Allow PostgreSQL, Crystal Forge server, and Grafana ports
        networking.firewall.allowedTCPPorts = [5432 8000 3000];

        virtualisation.writableStore = true;
        virtualisation.memorySize = 2048;
        virtualisation.cores = 2;

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
        };

        # Enable Crystal Forge server
        services.crystal-forge = {
          enable = true;
          local-database = true;
          log_level = "info";

          server = {
            enable = true;
            host = "127.0.0.1";
            port = 8000;
          };

          # Disable components not needed for Grafana dashboard testing
          build.enable = false;
          client.enable = false;

          # Database configuration
          database = {
            host = "localhost";
            user = "crystal_forge";
            name = "crystal_forge";
            port = 5432;
          };

          # Enable Grafana dashboard support
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

        # Configure Grafana
        services.grafana = {
          enable = true;
          settings = {
            server = {
              http_addr = "127.0.0.1";
              http_port = 3000;
              root_url = "http://127.0.0.1:3000";
            };
            security = {
              admin_user = "admin";
              admin_password = "admin";
            };
            database = {
              type = "postgres";
              host = "127.0.0.1:5432";
              name = "crystal_forge";
              user = "grafana";
              # password not needed with trust auth
            };
            users = {
              allow_sign_up = false;
            };
          };
        };

        environment.systemPackages = with pkgs; [
          git
          jq
          curl
          crystal-forge.default
          crystal-forge.cf-test-suite.runTests
          crystal-forge.cf-test-suite.testRunner
        ];

        environment.variables = {
          TMPDIR = "/tmp";
          TMP = "/tmp";
          TEMP = "/tmp";
        };

        environment.etc = {
          "agent.key".source = "${keyPath}/agent.key";
          "agent.pub".source = "${pubPath}/agent.pub";
        };
      };
    };

    globalTimeout = 150; # 10 minutes - Grafana takes time to start

    extraPythonPackages = p: [
      p.pytest
      p.pytest-xdist
      p.pytest-metadata
      p.pytest-html
      p.psycopg2
      p.requests
      pkgs.crystal-forge.cf-test-suite
    ];

    testScript = ''
      import os
      import pytest

      os.environ["NIXOS_TEST_DRIVER"] = "1"
      start_all()

      # Wait for PostgreSQL
      server.wait_for_unit("postgresql.service")
      server.wait_for_open_port(5432)

      # Forward DB to host for the python tests
      server.forward_port(5433, 5432)

      # Wait for Crystal Forge server
      # server.wait_for_unit("crystal-forge-server.service")
      # server.wait_for_open_port(8000)

      # Wait for Grafana - this takes a bit longer
      # print("⏳ Waiting for Grafana to start...")
      # server.wait_for_unit("grafana.service")
      # server.wait_for_open_port(3000)
      # print("✓ Grafana is ready")

      # Test env for client/fixtures
      os.environ["CF_TEST_DB_HOST"] = "127.0.0.1"
      os.environ["CF_TEST_DB_PORT"] = "5433"
      os.environ["CF_TEST_DB_USER"] = "postgres"
      os.environ["CF_TEST_DB_PASSWORD"] = ""
      os.environ["CF_TEST_SERVER_HOST"] = "127.0.0.1"
      os.environ["CF_TEST_SERVER_PORT"] = "8000"

      import cf_test
      cf_test._driver_machines = { "server": server }

      print("\n" + "="*60)
      print("Running Grafana Dashboard Tests")
      print("="*60 + "\n")

      exit_code = pytest.main([
          "-vvvv",
          "--tb=short",
          "-x",
          "-s",
          "-m", "dashboard",
          "--pyargs", "cf_test",
      ])
      if exit_code != 0:
          raise SystemExit(exit_code)
    '';
  }

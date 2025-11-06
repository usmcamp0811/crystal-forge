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
    name = "crystal-forge-database-test";
    skipLint = true;
    skipTypeCheck = true;

    nodes = {
      server = {
        imports = [inputs.self.nixosModules.crystal-forge];

        networking.useDHCP = true;
        networking.firewall.allowedTCPPorts = [CF_TEST_SERVER_PORT 5432];

        virtualisation.writableStore = true;

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
          curl
          crystal-forge.default
          crystal-forge.default.migrate # provides `crystal-forge-migrate`
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

    globalTimeout = 120;

    extraPythonPackages = p: [
      p.pytest
      p.pytest-xdist
      p.pytest-metadata
      p.pytest-html
      p.psycopg2
      pkgs.crystal-forge.cf-test-suite
    ];

    testScript = ''
      import os
      import pytest

      os.environ["NIXOS_TEST_DRIVER"] = "1"
      start_all()

      server.wait_for_unit("postgresql.service")
      server.wait_for_open_port(5432)

      # --- Run DB migrations inside the VM (before exposing the port to host)
      server.succeed(
        "DATABASE_URL='postgresql://postgres@127.0.0.1:5432/crystal_forge' crystal-forge-migrate"
      )

      # Forward DB to host for the python tests
      server.forward_port(5433, 5432)

      # Test env for client/fixtures
      os.environ["CF_TEST_DB_HOST"] = "127.0.0.1"
      os.environ["CF_TEST_DB_PORT"] = "5433"
      os.environ["CF_TEST_DB_USER"] = "postgres"
      os.environ["CF_TEST_DB_PASSWORD"] = ""
      os.environ["CF_TEST_SERVER_HOST"] = "127.0.0.1"
      os.environ["CF_TEST_SERVER_PORT"] = "${toString CF_TEST_SERVER_PORT}"

      import cf_test
      cf_test._driver_machines = { "server": server }

      exit_code = pytest.main([
          "-vvvv",
          "--tb=short",
          "-x",
          "-s",
          "-m", "database",
          "--pyargs", "cf_test",
      ])
      if exit_code != 0:
          raise SystemExit(exit_code)
    '';
  }

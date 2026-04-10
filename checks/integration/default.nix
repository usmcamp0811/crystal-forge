{ lib, inputs, pkgs, ... }:
let
  keyPair = pkgs.runCommand "agent-keypair" { } ''
    mkdir -p $out
    ${pkgs.crystal-forge.default.cf-keygen}/bin/cf-keygen -f $out/agent.key
  '';
  keyPath = pkgs.runCommand "agent.key" { } ''
    mkdir -p $out
    cp ${keyPair}/agent.key $out/
  '';
  pubPath = pkgs.runCommand "agent.pub" { } ''
    mkdir -p $out
    cp ${keyPair}/agent.pub $out/
  '';
  derivation-paths = lib.crystal-forge.derivation-paths pkgs;
  CF_TEST_DB_PORT = 5432;
  # Port 8000 for Crystal Forge server; Grafana occupies 3000 by default.
  CF_TEST_SERVER_PORT = 8000;
  GRAFANA_PORT = 3000;
  systemBuildClosure = pkgs.closureInfo {
    rootPaths = [
      inputs.self.nixosConfigurations.cf-test-sys.config.system.build.toplevel
      pkgs.crystal-forge.default
      pkgs.path
    ] ++ lib.crystal-forge.prefetchedPaths;
  };
in pkgs.testers.runNixOSTest {
  name = "crystal-forge-integration-test";
  skipLint = true;
  skipTypeCheck = true;

  nodes = {
    gitserver = lib.crystal-forge.makeGitServerNode {
      inherit pkgs systemBuildClosure;
      port = 8080;
    };

    server = {
      nix.settings = {
        experimental-features = [ "nix-command" "flakes" ];
      };
      imports = [ inputs.self.nixosModules.crystal-forge ];

      networking.useDHCP = true;
      networking.firewall.allowedTCPPorts = [ CF_TEST_SERVER_PORT GRAFANA_PORT 5432 ];

      virtualisation.writableStore = true;
      virtualisation.memorySize = 8096;
      virtualisation.cores = 4;
      virtualisation.diskSize = 16384;
      virtualisation.additionalPaths = [
        systemBuildClosure
        inputs.self.nixosConfigurations.cf-test-sys.config.system.build.toplevel.drvPath
        inputs.nixpkgs.outPath
      ] ++ lib.crystal-forge.prefetchedPaths;

      systemd.tmpfiles.rules = [
        "d /var/lib/crystal-forge 0755 crystal-forge crystal-forge -"
        "d /var/lib/crystal-forge/.cache 0755 crystal-forge crystal-forge -"
        "d /var/lib/crystal-forge/.cache/nix 0755 crystal-forge crystal-forge -"
        "Z /var/lib/crystal-forge/.cache/nix - crystal-forge crystal-forge -"
      ];

      # local-database = true (below) and dashboards.enable = true both contribute
      # to services.postgresql.initialScript via the crystal-forge module.
      # Do not set initialScript here to avoid conflicting definitions.
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

      environment.systemPackages = with pkgs; [
        git
        jq
        hello
        openssl
        curl
        crystal-forge.default
        crystal-forge.default.migrate
        crystal-forge.cf-test-suite.runTests
        crystal-forge.cf-test-suite.testRunner
      ];

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
          server_port = CF_TEST_SERVER_PORT;
          private_key = "/etc/server.key";
        };

        database = {
          host = "localhost";
          user = "crystal_forge";
          name = "crystal_forge";
          port = 5432;
        };

        server = {
          enable = true;
          port = CF_TEST_SERVER_PORT;
          host = "0.0.0.0";
        };

        # Builder is enabled so the service exists; it is stopped at test startup
        # and only restarted for the builder test phase to prevent interference.
        build = {
          enable = true;
          offline = false;
        };

        cache = {
          push_after_build = false;
          push_to = null;
        };

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

        flakes = {
          flake_polling_interval = "1m";
          watched = [{
            name = "test-flake";
            repo_url = "http://gitserver/crystal-forge";
            branch = "main";
            auto_poll = true;
            initial_commit_depth = 5;
          }];
        };

        environments = [{
          name = "test";
          description = "Test environment for Crystal Forge integration test";
          is_active = true;
          risk_profile = "LOW";
          compliance_level = "NONE";
        }];

        systems = [{
          hostname = "server";
          public_key = lib.strings.trim (builtins.readFile "${pubPath}/agent.pub");
          environment = "test";
          flake_name = "test-flake";
        }];
      };
    };
  };

  # 30 minutes: database (120s) + dashboard (150s) + server (600s) + builder (300s) + overhead
  globalTimeout = 1800;

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

    # --- Infrastructure setup ---
    server.wait_for_unit("postgresql.service")
    server.wait_for_open_port(5432)

    # Stop builder immediately; it will be started again for the builder test phase.
    server.succeed("systemctl stop crystal-forge-builder.service || true")

    # Run DB migrations explicitly (required for -m database tests).
    server.succeed(
      "DATABASE_URL='postgresql://postgres@127.0.0.1:5432/crystal_forge' crystal-forge-migrate"
    )

    server.wait_for_unit("crystal-forge-server.service")
    server.wait_for_open_port(${toString CF_TEST_SERVER_PORT})

    # Wait for Grafana (needed for -m dashboard tests).
    print("Waiting for Grafana to start...")
    server.wait_for_unit("grafana.service")
    server.wait_for_open_port(${toString GRAFANA_PORT})
    server.succeed(
      "curl --fail http://127.0.0.1:${toString GRAFANA_PORT}/api/health"
      " || (echo 'Grafana health check failed' >&2; exit 1)"
    )
    print("Grafana ready")

    # Forward ports to host for Python test access.
    server.forward_port(5433, 5432)
    server.forward_port(${toString CF_TEST_SERVER_PORT}, ${toString CF_TEST_SERVER_PORT})
    server.forward_port(${toString GRAFANA_PORT}, ${toString GRAFANA_PORT})

    from cf_test.vm_helpers import wait_for_git_server_ready
    wait_for_git_server_ready(gitserver, timeout=120)

    # Read commit metadata from test flake.
    main_head = "${lib.strings.trim (builtins.readFile (lib.crystal-forge.testFlake + "/MAIN_HEAD"))}"
    dev_head = "${lib.strings.trim (builtins.readFile (lib.crystal-forge.testFlake + "/DEVELOPMENT_HEAD"))}"
    feature_head = "${lib.strings.trim (builtins.readFile (lib.crystal-forge.testFlake + "/FEATURE_HEAD"))}"

    main_commits = """${lib.strings.trim (builtins.readFile (lib.crystal-forge.testFlake + "/MAIN_COMMITS"))}"""
    dev_commits = """${lib.strings.trim (builtins.readFile (lib.crystal-forge.testFlake + "/DEVELOPMENT_COMMITS"))}"""
    feature_commits = """${lib.strings.trim (builtins.readFile (lib.crystal-forge.testFlake + "/FEATURE_COMMITS"))}"""

    os.environ.update({
      "CF_TEST_GIT_SERVER_URL": "http://gitserver/crystal-forge",
      "CF_TEST_REAL_REPO_URL": "http://gitserver/crystal-forge",
      "CF_TEST_REAL_COMMIT_HASH": main_head,
      "CF_TEST_MAIN_HEAD": main_head,
      "CF_TEST_DEVELOPMENT_HEAD": dev_head,
      "CF_TEST_FEATURE_HEAD": feature_head,
      "CF_TEST_MAIN_COMMITS": main_commits.replace('\n', ','),
      "CF_TEST_DEVELOPMENT_COMMITS": dev_commits.replace('\n', ','),
      "CF_TEST_FEATURE_COMMITS": feature_commits.replace('\n', ','),
      "CF_TEST_MAIN_COMMIT_COUNT": "${lib.strings.trim (builtins.readFile (lib.crystal-forge.testFlake + "/MAIN_COMMIT_COUNT"))}",
      "CF_TEST_DEVELOPMENT_COMMIT_COUNT": "${lib.strings.trim (builtins.readFile (lib.crystal-forge.testFlake + "/DEVELOPMENT_COMMIT_COUNT"))}",
      "CF_TEST_FEATURE_COMMIT_COUNT": "${lib.strings.trim (builtins.readFile (lib.crystal-forge.testFlake + "/FEATURE_COMMIT_COUNT"))}",
      "CF_TEST_DB_HOST": "127.0.0.1",
      "CF_TEST_DB_PORT": "5433",
      "CF_TEST_DB_USER": "postgres",
      "CF_TEST_DB_PASSWORD": "",
      "CF_TEST_SERVER_HOST": "127.0.0.1",
      "CF_TEST_SERVER_PORT": "${toString CF_TEST_SERVER_PORT}",
      "CF_TEST_DRV": "${derivation-paths}",
      "CF_TEST_FLAKE_NAME": "test-flake",
      "CF_TEST_PRELOADED_FLAKE_PATH": "/etc/preloaded-flake",
    })

    import cf_test
    # Expose both keys: "server" (server/database/dashboard tests) and
    # "cfServer" (builder tests use this key).
    cf_test._driver_machines = {
      "server": server,
      "cfServer": server,
      "gitserver": gitserver,
    }

    # --- Phase 1: Database migration tests ---
    print("=== Phase 1: Database tests ===")
    exit_code = pytest.main([
      "-vvvv", "--tb=short", "-x", "-s",
      "-m", "database", "--pyargs", "cf_test",
    ])
    if exit_code != 0:
      raise SystemExit(exit_code)

    # --- Phase 2: Grafana dashboard tests ---
    print("=== Phase 2: Dashboard tests ===")
    print("Checking Grafana provisioning...")
    server.succeed(
      "curl -sS -u admin:admin http://127.0.0.1:${toString GRAFANA_PORT}/api/datasources | jq . || echo 'API unavailable'"
    )
    server.succeed(
      "systemctl status crystal-forge-grafana-db-init || true"
    )

    exit_code = pytest.main([
      "-vvvv", "--tb=short", "-x", "-s",
      "-m", "dashboard", "--pyargs", "cf_test",
    ])
    if exit_code != 0:
      raise SystemExit(exit_code)

    # --- Phase 3: Server tests ---
    print("=== Phase 3: Server tests ===")
    exit_code = pytest.main([
      "-vvvv", "--tb=short", "-x", "-s",
      "-m", "server", "--pyargs", "cf_test",
    ])
    if exit_code != 0:
      raise SystemExit(exit_code)

    # --- Phase 4: Builder tests ---
    # Start the builder now that server tests are done to prevent interference.
    print("=== Phase 4: Builder tests ===")
    server.succeed("systemctl start crystal-forge-builder.service")
    server.wait_for_unit("crystal-forge-builder.service")

    exit_code = pytest.main([
      "-vvvv", "--tb=short", "-x", "-s",
      "-m", "builder", "--pyargs", "cf_test",
    ])
    if exit_code != 0:
      raise SystemExit(exit_code)

    print("All integration tests passed.")
  '';
}

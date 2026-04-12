# Mega Web UI Integration Test
#
# Comprehensive integration test that combines:
# - Full stack: PostgreSQL, Crystal Forge server, gitserver
# - Cache backends: Attic + S3/MinIO
# - Builder service with real builds
# - Web UI with Playwright browser tests
#
# This consolidates what were previously 5 separate VM checks (attic_cache,
# s3_cache, builder, and web-ui) into one comprehensive integration test,
# significantly reducing CI time.
#
# Test phases:
# 1. Cache tests (Attic + S3)
# 2. Builder tests
# 3. Web UI tests (Playwright screenshots)
#
# Note: OIDC tests remain in the separate integration check.
#
{ lib, pkgs, inputs, ... }:
let
  testDir = ./tests;
  CF_TEST_SERVER_PORT = 3000;

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
  systemBuildClosure = pkgs.closureInfo {
    rootPaths = [
      inputs.self.nixosConfigurations.cf-test-sys.config.system.build.toplevel
      pkgs.crystal-forge.default
      pkgs.path
    ] ++ lib.crystal-forge.prefetchedPaths;
  };
in pkgs.testers.runNixOSTest {
  name = "crystal-forge-web-ui-mega-integration";

  skipLint = true;
  skipTypeCheck = true;

  nodes = {
    # Git server for flake testing
    gitserver = lib.crystal-forge.makeGitServerNode {
      inherit pkgs systemBuildClosure;
      port = 8080;
    };

    # Attic binary cache
    atticCache = lib.crystal-forge.makeAtticCacheNode {
      inherit lib pkgs;
      port = 8080;
      jwtSecretB64 = "dGVzdCBzZWNyZXQgZm9yIGF0dGljZA==";
    };

    # S3-compatible cache (MinIO)
    s3Cache = lib.crystal-forge.makeS3CacheNode {
      inherit lib pkgs;
      port = 9000;
      region = "us-east-1";
      bucket = "nix-cache";
    };

    # Main Crystal Forge server with all services enabled
    machine = {
      imports = [ inputs.self.nixosModules.crystal-forge ];

      virtualisation.memorySize = 20480; # 20GB for everything
      virtualisation.cores = 4;
      virtualisation.diskSize = 40960;
      virtualisation.writableStore = true;
      virtualisation.additionalPaths = [
        systemBuildClosure
        inputs.self.nixosConfigurations.cf-test-sys.config.system.build.toplevel.drvPath
        inputs.nixpkgs.outPath
      ] ++ lib.crystal-forge.prefetchedPaths;

      environment.systemPackages = [
        pkgs.chromium
        pkgs.nodejs
        pkgs.playwright-test
        pkgs.curl
        pkgs.jq
        pkgs.git
        pkgs.crystal-forge.default
        pkgs.crystal-forge.default.migrate
        pkgs.crystal-forge.cf-test-suite.runTests
        pkgs.crystal-forge.cf-test-suite.testRunner
      ];

      environment.variables = {
        NODE_PATH = "${pkgs.playwright-test}/lib/node_modules";
        PLAYWRIGHT_BROWSERS_PATH = "${pkgs.playwright-driver.browsers}";
        TMPDIR = "/tmp";
        TMP = "/tmp";
        TEMP = "/tmp";
      };

      environment.etc = {
        "server.key".source = "${keyPath}/agent.key";
        "server.pub".source = "${pubPath}/agent.pub";
      };

      networking.firewall.allowedTCPPorts = [ CF_TEST_SERVER_PORT 5432 ];

      systemd.tmpfiles.rules = [
        "d /var/lib/crystal-forge 0755 crystal-forge crystal-forge -"
        "d /var/lib/crystal-forge/.cache 0755 crystal-forge crystal-forge -"
        "d /var/lib/crystal-forge/.cache/nix 0755 crystal-forge crystal-forge -"
        "Z /var/lib/crystal-forge/.cache/nix - crystal-forge crystal-forge -"
      ];

      # PostgreSQL
      services.postgresql = {
        enable = true;
        settings = {
          "listen_addresses" = lib.mkForce "*";
          "fsync" = "off";
          "synchronous_commit" = "off";
          "full_page_writes" = "off";
          "max_wal_size" = "64MB";
          "min_wal_size" = "32MB";
        };
        authentication = lib.concatStringsSep "\n" [
          "local   all   postgres   trust"
          "local   all   all        peer"
          "host    all   all 127.0.0.1/32 trust"
          "host    all   all ::1/128      trust"
          "host    all   all 10.0.2.2/32  trust"
        ];
      };

      # Crystal Forge - starts with local auth, will switch to OIDC during test
      services.crystal-forge = {
        enable = true;
        local-database = true;
        # Builder startup tests assert INFO-level startup logs.
        # Keep INFO here so those assertions remain observable in the mega check.
        log_level = "info";

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

        build = {
          enable = true;
          offline = false;
        };

        cache = {
          push_after_build = false;
          push_to = null;
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
          description = "Test environment for mega integration";
          is_active = true;
          risk_profile = "LOW";
          compliance_level = "NONE";
        }];

        systems = [{
          hostname = "mega-test-system";
          public_key =
            lib.strings.trim (builtins.readFile "${pubPath}/agent.pub");
          environment = "test";
          flake_name = "test-flake";
        }];
      };

      systemd.services."crystal-forge-postgres-jobs".enable = lib.mkForce false;
      systemd.timers."crystal-forge-postgres-jobs".enable = lib.mkForce false;

      # Start with local auth
      systemd.services.crystal-forge-server.environment.AUTH_MODE = "local";
    };
  };

  globalTimeout = 2400; # 40 minutes for comprehensive testing

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
    import json
    import os
    import pytest

    os.environ["NIXOS_TEST_DRIVER"] = "1"

    start_all()

    # === Infrastructure Warmup ===
    print("=== Infrastructure Warmup ===")
    machine.wait_for_unit("postgresql.service")
    machine.wait_for_unit("crystal-forge-server.service")
    machine.wait_for_unit("crystal-forge-builder.service")
    machine.wait_for_open_port(${toString CF_TEST_SERVER_PORT})
    machine.wait_for_open_port(5432)

    from cf_test.vm_helpers import wait_for_git_server_ready
    wait_for_git_server_ready(gitserver, timeout=120)

    atticCache.wait_for_unit("atticd.service")
    atticCache.wait_for_open_port(8080)

    s3Cache.wait_for_unit("minio.service")
    s3Cache.wait_for_open_port(9000)

    # Set up test environment variables
    main_head = "${
      lib.strings.trim
      (builtins.readFile (lib.crystal-forge.testFlake + "/MAIN_HEAD"))
    }"

    os.environ.update({
      "CF_TEST_GIT_SERVER_URL": "http://gitserver/crystal-forge",
      "CF_TEST_REAL_REPO_URL": "http://gitserver/crystal-forge",
      "CF_TEST_REAL_COMMIT_HASH": main_head,
      "CF_TEST_DB_HOST": "127.0.0.1",
      "CF_TEST_DB_PORT": "5433",
      "CF_TEST_DB_USER": "postgres",
      "CF_TEST_DB_PASSWORD": "",
      "CF_TEST_SERVER_HOST": "127.0.0.1",
      "CF_TEST_SERVER_PORT": "${toString CF_TEST_SERVER_PORT}",
      "CF_TEST_DRV": "${derivation-paths}",
      "CF_TEST_FLAKE_NAME": "test-flake",
    })

    machine.forward_port(5433, 5432)
    machine.forward_port(${toString CF_TEST_SERVER_PORT}, ${
      toString CF_TEST_SERVER_PORT
    })

    import cf_test
    cf_test._driver_machines = {
      "machine": machine,
      "cfServer": machine,
      "gitserver": gitserver,
      "atticCache": atticCache,
      "s3Cache": s3Cache,
    }

    # === Phase 1: Attic Cache Tests ===
    print("=== Phase 1: Attic Cache Tests ===")
    exit_code = pytest.main([
      "-vvvv", "--tb=short", "-x", "-s",
      "-m", "attic_cache", "--pyargs", "cf_test",
    ])
    if exit_code != 0:
      raise SystemExit(exit_code)

    # === Phase 2: S3 Cache Tests ===
    print("=== Phase 2: S3 Cache Tests ===")
    exit_code = pytest.main([
      "-vvvv", "--tb=short", "-x", "-s",
      "-m", "s3cache", "--pyargs", "cf_test",
    ])
    if exit_code != 0:
      raise SystemExit(exit_code)

    # === Phase 3: Builder Tests ===
    print("=== Phase 3: Builder Tests ===")
    exit_code = pytest.main([
      "-vvvv", "--tb=short", "-x", "-s",
      "-m", "builder", "--pyargs", "cf_test",
    ])
    if exit_code != 0:
      raise SystemExit(exit_code)

    # === Phase 4: Web UI Tests (Playwright) ===
    print("=== Phase 4: Web UI Tests (Playwright) ===")

    # Verify server is responding
    machine.succeed("curl -sf http://127.0.0.1:${
      toString CF_TEST_SERVER_PORT
    }/status | jq .")
    print("Server is up and responding")

    # Create output directories
    machine.succeed("mkdir -p /tmp/screenshots")
    machine.succeed("mkdir -p /tmp/web-ui-tests")

    # Copy test files into VM
    machine.succeed("cp -r ${testDir}/* /tmp/web-ui-tests/")

    test_profile = "ci_fast"

    # Run the integration test script
    machine.succeed(
        f"nohup env CF_UI_TEST_PROFILE={test_profile} ${pkgs.nodejs}/bin/node /tmp/web-ui-tests/integration-test.js http://127.0.0.1:${
          toString CF_TEST_SERVER_PORT
        } /tmp/screenshots > /tmp/web-ui-tests/integration.log 2>&1 </dev/null &"
    )
    machine.wait_until_succeeds("test -f /tmp/screenshots/results.json", timeout=1800)
    output = machine.succeed("cat /tmp/web-ui-tests/integration.log")
    print(output)

    # Read results
    results_json = machine.succeed("cat /tmp/screenshots/results.json")
    results = json.loads(results_json)

    # Copy screenshots out
    for r in results:
        if r.get("ok"):
            machine.copy_from_vm(f"/tmp/screenshots/{r['name']}.png", "screenshots")

    ok_count = sum(1 for r in results if r.get("ok"))

    print("\n=== Summary ===")
    print(f"  Screenshots: {ok_count}/{len(results)} captured")

    for r in results:
        status = "OK" if r.get("ok") else "FAIL"
        error = r.get("error", "")
        if error:
            print(f"  [{status}] {r['name']} - {error}")
        else:
            print(f"  [{status}] {r['name']}")

    if ok_count == 0:
        raise Exception("All screenshots failed")

    # Fail if critical tests failed
    critical_tests = [
      "01-login-page",
      "02-registration",
      "05-login-submit",
      "06-dashboard",
      "06x-pipeline-readiness-scroll",
      "06y-recent-deployments-scroll",
      "06z-fleet-health-widget-assert",
      "15-builds",
      "11b-builds-queue-card-focus",
      "15h-builds-completed-restart-action",
      "12c-systems-modal-config-field",
      "12e-systems-edit-modal",
      "12f-systems-deploy-modal",
      "12g-system-detail-history-logs-edit",
      "13e-flakes-add-modal-credentials",
      "13f-flakes-edit-modal-credentials",
      "16-cves",
      "16b-cves-severity-filter",
    ]
    failed_critical = [r['name'] for r in results if r['name'] in critical_tests and not r.get('ok')]
    if failed_critical:
        raise Exception(f"Critical web UI checks failed: {failed_critical}")

    print("\n=== All Mega Integration Tests Passed ===")
    print("Completed: Cache (Attic+S3), Builder, Web UI")
  '';
}

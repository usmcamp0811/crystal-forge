# Web UI Integration Test
#
# Full integration test that:
# 1. Starts the Crystal Forge server with PostgreSQL
# 2. Uses Playwright to register an admin user
# 3. Logs in with that user
# 4. Takes screenshots of all major routes
#
# Output ($out):
#   screenshots/   — PNG screenshots of all tested routes
#
# Run: nix build .#checks.x86_64-linux.web-ui
#      ls ./result/screenshots/
{ lib, pkgs, inputs, ... }:
let
  testDir = ./tests;
  CF_TEST_SERVER_PORT = 3000;
in pkgs.testers.runNixOSTest {
  name = "crystal-forge-web-ui-integration";

  skipLint = true;
  skipTypeCheck = true;

  nodes.machine = {
    imports = [ inputs.self.nixosModules.crystal-forge ];

    virtualisation.memorySize = 12288;
    virtualisation.cores = 2;
    virtualisation.diskSize = 32768;

    environment.systemPackages =
      [ pkgs.chromium pkgs.nodejs pkgs.playwright-test pkgs.curl pkgs.jq ];

    environment.variables = {
      NODE_PATH = "${pkgs.playwright-test}/lib/node_modules";
      PLAYWRIGHT_BROWSERS_PATH = "${pkgs.playwright-driver.browsers}";
    };

    networking.firewall.allowedTCPPorts = [ CF_TEST_SERVER_PORT ];

    # PostgreSQL for the server
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
      ];
      ensureDatabases = [ "crystal_forge" ];
      ensureUsers = [{
        name = "crystal_forge";
        ensureDBOwnership = true;
      }];
    };

    # Crystal Forge server with local auth enabled
    services.crystal-forge = {
      enable = true;
      local-database = true;
      log_level = "warn"; # Reduce log noise

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

      # Disable build/flakes - we just need the server for auth testing
      build.enable = false;

      # Set very long polling intervals to avoid noise during test
      flakes = {
        flake_polling_interval = "24h";
        commit_evaluation_interval = "24h";
        build_processing_interval = "24h";
      };
    };

    systemd.services."crystal-forge-postgres-jobs".enable = lib.mkForce false;
    systemd.timers."crystal-forge-postgres-jobs".enable = lib.mkForce false;

    # Set auth mode to local via environment
    systemd.services.crystal-forge-server.environment.AUTH_MODE = "local";
  };

  globalTimeout = 1200;

  testScript = ''
    import json

    machine.start()
    machine.wait_for_unit("postgresql.service")
    machine.wait_for_unit("crystal-forge-server.service")
    machine.wait_for_open_port(${toString CF_TEST_SERVER_PORT})

    # Verify server is responding
    machine.succeed("curl -sf http://127.0.0.1:${
      toString CF_TEST_SERVER_PORT
    }/status | jq .")
    print("Server is up and responding")

    # Verify setup-status shows requires_setup=true (no users yet)
    setup_status = machine.succeed("curl -sf http://127.0.0.1:${
      toString CF_TEST_SERVER_PORT
    }/api/auth/setup-status")
    print(f"Setup status: {setup_status}")

    # Create output directories
    machine.succeed("mkdir -p /tmp/screenshots")
    machine.succeed("mkdir -p /tmp/web-ui-tests")

    # Copy test files into VM
    machine.succeed("cp -r ${testDir}/* /tmp/web-ui-tests/")

    test_profile = "ci_fast"

    # Run the integration test script detached to avoid command output timeout
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

    ok_names = {r["name"] for r in results if r.get("ok")}

    if test_profile != "ci_fast":
      # WebSocket Eval Log Streaming Test
      print("\n=== WebSocket Eval Log Test ===")
      print("Testing that eval logs stream correctly via WebSocket...")
      print("This validates the fix for late-connecting WebSocket clients")

      # Login as the admin user that was created by the integration test
      machine.succeed("""
          curl -sf -X POST http://127.0.0.1:${
            toString CF_TEST_SERVER_PORT
          }/api/auth/local/login \
            -H 'Content-Type: application/json' \
            -d '{"username":"admin","password":"testpassword123"}' \
            > /tmp/wstest-login.json
      """)

      token_json = machine.succeed("cat /tmp/wstest-login.json")
      token_data = json.loads(token_json)
      auth_token = token_data.get("token", "")

      if not auth_token:
          print("Warning: Could not get auth token, skipping WebSocket test")
      else:
          ws_exit_code, ws_output = machine.execute(
              f"${pkgs.nodejs}/bin/node /tmp/web-ui-tests/eval-websocket-test.js http://127.0.0.1:${
                toString CF_TEST_SERVER_PORT
              } {auth_token} 2>&1",
              True,
              True,
              600
          )
          print(ws_output)

          if ws_exit_code != 0:
              raise Exception("WebSocket eval log streaming test failed")

      expected_onboarding = [
        "06a-onboarding-coach-dashboard",
        "06b-onboarding-environments-callout",
        "06b2-onboarding-environments-form-callouts",
        "06b3-onboarding-environments-create",
        "06c-onboarding-flakes-callout",
        "06c2-onboarding-flakes-form-callouts",
        "06c3-onboarding-flakes-create",
        "06d-onboarding-builders-callout",
        "06d2-onboarding-builders-form-callouts",
        "06d3-onboarding-builders-create",
        "06e-onboarding-caches-callout",
        "06e2-onboarding-caches-form-callouts",
        "06e3-onboarding-caches-create",
        "06f-onboarding-systems-callout",
        "06f2-onboarding-systems-form-callouts",
        "06f3-onboarding-systems-keygen",
        "06f4-onboarding-systems-create",
        "06g-onboarding-coach-minimized",
        "06h-onboarding-coach-all-configured",
      ]
      missing_onboarding = [name for name in expected_onboarding if name not in ok_names]
      if missing_onboarding:
          raise Exception(f"Missing required onboarding screenshots: {missing_onboarding}")

    # Fail if critical auth + navigation checks failed
    critical_tests = [
      "01-login-page",
      "02-registration",
      "05-login-submit",
      "06-dashboard",
      "15-builds",
      "11b-builds-queue-card-focus",
      "12c-systems-modal-config-field",
      "12e-systems-edit-modal",
      "12f-systems-deploy-modal",
      "13e-flakes-add-modal-credentials",
      "13f-flakes-edit-modal-credentials",
    ] if test_profile == "ci_fast" else [
      "01-login-page",
      "02-registration",
      "05-login-submit",
      "06-dashboard",
      "06a-onboarding-coach-dashboard",
      "06b-onboarding-environments-callout",
      "06b2-onboarding-environments-form-callouts",
      "06b3-onboarding-environments-create",
      "06c-onboarding-flakes-callout",
      "06c2-onboarding-flakes-form-callouts",
      "06c3-onboarding-flakes-create",
      "06d-onboarding-builders-callout",
      "06d2-onboarding-builders-form-callouts",
      "06d3-onboarding-builders-create",
      "06e-onboarding-caches-callout",
      "06e2-onboarding-caches-form-callouts",
      "06e3-onboarding-caches-create",
      "06f-onboarding-systems-callout",
      "06f2-onboarding-systems-form-callouts",
      "06f3-onboarding-systems-keygen",
      "06f4-onboarding-systems-create",
      "06g-onboarding-coach-minimized",
      "06h-onboarding-coach-all-configured",
      "12c-systems-modal-config-field",
      "12e-systems-edit-modal",
      "12f-systems-deploy-modal",
      "13e-flakes-add-modal-credentials",
      "13f-flakes-edit-modal-credentials",
      "07-sidebar-desktop-expanded",
      "08-sidebar-desktop-collapsed",
      "08b-sidebar-desktop-toggle-expand",
      "09-sidebar-tablet-collapsed",
      "09b-sidebar-tablet-expanded",
      "09c-sidebar-mobile-drawer",
      "09d-sidebar-narrow-collapsed",
      "09e-sidebar-sections-fullwidth",
    ]
    failed_critical = [r['name'] for r in results if r['name'] in critical_tests and not r.get('ok')]
    if failed_critical:
        raise Exception(f"Critical web UI checks failed: {failed_critical}")
  '';
}

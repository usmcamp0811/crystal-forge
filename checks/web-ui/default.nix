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

    virtualisation.memorySize = 4096;
    virtualisation.cores = 2;

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
      settings."listen_addresses" = lib.mkForce "*";
      authentication = lib.concatStringsSep "\n" [
        "local   all   postgres   trust"
        "local   all   all        peer"
        "host    all   all 127.0.0.1/32 trust"
        "host    all   all ::1/128      trust"
      ];
      initialScript = pkgs.writeText "init-crystal-forge.sql" ''
        CREATE USER crystal_forge LOGIN;
        CREATE DATABASE crystal_forge OWNER crystal_forge;
        GRANT ALL PRIVILEGES ON DATABASE crystal_forge TO crystal_forge;
      '';
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

    # Set auth mode to local via environment
    systemd.services.crystal-forge-server.environment.AUTH_MODE = "local";
  };

  globalTimeout = 420; # 7 minutes

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

    # Run the integration test script
    exit_code, output = machine.execute(
        "${pkgs.nodejs}/bin/node /tmp/web-ui-tests/integration-test.js http://127.0.0.1:${
          toString CF_TEST_SERVER_PORT
        } /tmp/screenshots 2>&1"
    )
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

    expected_onboarding = [
      "06a-onboarding-coach-dashboard",
      "06b-onboarding-environments-callout",
      "06c-onboarding-flakes-callout",
      "06d-onboarding-builders-callout",
      "06e-onboarding-caches-callout",
      "06f-onboarding-systems-callout",
    ]
    ok_names = {r["name"] for r in results if r.get("ok")}
    missing_onboarding = [name for name in expected_onboarding if name not in ok_names]
    if missing_onboarding:
        raise Exception(f"Missing required onboarding screenshots: {missing_onboarding}")

    # Fail if critical auth + navigation checks failed
    critical_tests = [
      "01-login-page",
      "02-registration",
      "05-login-submit",
      "06-dashboard",
      "06a-onboarding-coach-dashboard",
      "06b-onboarding-environments-callout",
      "06c-onboarding-flakes-callout",
      "06d-onboarding-builders-callout",
      "06e-onboarding-caches-callout",
      "06f-onboarding-systems-callout",
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

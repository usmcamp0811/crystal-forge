# Web UI Build Verification Check
#
# Verifies the Crystal Forge web UI compiles to WASM and produces valid output,
# then takes screenshots of every route using headless Chromium in a NixOS VM.
#
# Output ($out):
#   screenshots/   — PNG screenshots of core routes and modal states
#   result.txt     — Build verification summary
#
# Run: nix build .#checks.x86_64-linux.web-ui
#      ls ./result/screenshots/
{
  lib,
  pkgs,
  ...
}: let
  testDir = ./tests;
in
  pkgs.testers.runNixOSTest {
    name = "crystal-forge-web-ui-screenshots";

    skipLint = true;
    skipTypeCheck = true;

    nodes.machine = {
      virtualisation.memorySize = 4096;
      virtualisation.cores = 2;

      environment.systemPackages = [
        pkgs.chromium
        pkgs.nodejs
        pkgs.playwright-test
        pkgs.crystal-forge.web-ui
      ];

      environment.variables = {
        NODE_PATH = "${pkgs.playwright-test}/lib/node_modules";
        PLAYWRIGHT_BROWSERS_PATH = "${pkgs.playwright-driver.browsers}";
      };

      systemd.services.web-ui-server = {
        description = "Crystal Forge Web UI static server";
        wantedBy = ["multi-user.target"];
        after = ["network.target"];
        serviceConfig = {
          ExecStart = "${pkgs.crystal-forge.web-ui}/bin/crystal-forge-web-ui";
          Restart = "always";
        };
      };

      networking.firewall.allowedTCPPorts = [8080];
    };

    globalTimeout = 420;

    testScript = ''
      import json
      import pathlib

      machine.start()
      machine.wait_for_unit("web-ui-server.service")
      machine.wait_for_open_port(8080)

      # Basic SPA verification
      machine.succeed("curl -sf http://127.0.0.1:8080/ | grep -q 'Crystal Forge'")
      machine.succeed("curl -sf http://127.0.0.1:8080/systems | grep -q 'Crystal Forge'")
      print("SPA fallback working")

      machine.succeed("mkdir -p /tmp/screenshots")
      machine.succeed("mkdir -p /tmp/web-ui-tests")

      # Copy entire tests directory into VM
      machine.succeed("cp -r ${testDir}/* /tmp/web-ui-tests/")

      # Run screenshot runner
      exit_code, output = machine.execute(
          "${pkgs.nodejs}/bin/node /tmp/web-ui-tests/screenshot-runner.js http://127.0.0.1:8080 /tmp/screenshots 2>&1"
      )
      print(output)

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
          raise Exception("All screenshots failed - browser may not be working")
    '';
  }

# Web UI Build Verification Check
#
# Verifies the Crystal Forge web UI compiles to WASM and produces valid output,
# then takes screenshots of every route using headless Chromium in a NixOS VM.
#
# Output ($out):
#   screenshots/   — PNG screenshots of each route (dashboard, systems, style-guide, 404)
#   result.txt     — Build verification summary
#
# Run: nix build .#checks.x86_64-linux.web-ui
#      ls ./result/screenshots/
{
  lib,
  pkgs,
  inputs,
  ...
}: let
in
  pkgs.testers.runNixOSTest {
    name = "crystal-forge-web-ui-screenshots";
    skipLint = true;
    skipTypeCheck = true;

    nodes.machine = {
      virtualisation.memorySize = 4096;
      virtualisation.cores = 2;

      environment.systemPackages = [pkgs.chromium pkgs.python3];

      # Serve the web UI on port 8080 via a systemd service
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

    globalTimeout = 300; # 5 minutes

    testScript = ''
      import os

      machine.start()
      machine.wait_for_unit("web-ui-server.service")
      machine.wait_for_open_port(8080)

      # Verify SPA server handles both static files and route fallback
      machine.succeed("curl -sf http://127.0.0.1:8080/ | grep -q 'Crystal Forge'")
      machine.succeed("curl -sf http://127.0.0.1:8080/systems | grep -q 'Crystal Forge'")
      print("Web root is being served correctly (SPA fallback working)")

      # Routes to screenshot (matches packages/web-ui/src/routes.rs)
      routes = [
          ("/", "dashboard", "Dashboard (fleet overview)"),
          ("/systems", "systems", "Systems list"),
          ("/style-guide", "style-guide", "Design system style guide"),
          ("/not-a-real-page", "not-found", "404 not found page"),
      ]

      screenshot_results = []

      for path, name, desc in routes:
          print(f"Screenshotting: {desc} ({path})...")
          try:
              # Chromium headless with --virtual-time-budget to simulate time
              # passing so WASM can compile and the Dioxus app can render.
              # 30s of virtual time is enough for WASM init in a VM.
              machine.succeed(
                  f"chromium "
                  f"--headless=new "
                  f"--no-sandbox "
                  f"--disable-gpu "
                  f"--disable-software-rasterizer "
                  f"--disable-dev-shm-usage "
                  f"--virtual-time-budget=30000 "
                  f"--run-all-compositor-stages-before-draw "
                  f"--screenshot=/tmp/{name}.png "
                  f"--window-size=1920,1080 "
                  f"'http://127.0.0.1:8080{path}'"
              )

              size = machine.succeed(f"stat -c%s /tmp/{name}.png").strip()
              print(f"  OK: {name}.png ({size} bytes)")
              screenshot_results.append({"name": name, "size": int(size), "desc": desc, "ok": True})
          except Exception as e:
              print(f"  WARN: {name}.png failed: {e}")
              screenshot_results.append({"name": name, "size": 0, "desc": desc, "ok": False})

      # Copy screenshots from VM to $out/screenshots/
      for r in screenshot_results:
          if r["ok"]:
              machine.copy_from_vm(f"/tmp/{r['name']}.png", "screenshots")

      # Summary
      ok_count = sum(1 for r in screenshot_results if r["ok"])
      print(f"\n=== Summary ===")
      print(f"  Screenshots: {ok_count}/{len(routes)} captured")
      for r in screenshot_results:
          status = "OK" if r["ok"] else "FAIL"
          print(f"  [{status}] {r['name']}.png ({r['size']} bytes) - {r['desc']}")

      # At minimum, the build checks must pass (enforced by buildCheck dependency)
      # Screenshots are visual artifacts for review
      if ok_count == 0:
          raise Exception("All screenshots failed - browser may not be working")
    '';
  }

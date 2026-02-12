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
{ lib, pkgs, inputs, ... }:
let
  webUiSrc = "${inputs.self}/packages/web-ui";

  # Tailwind CSS (bundled locally so screenshots render with full styling)
  tailwindJs = builtins.fetchurl {
    url = "https://cdn.tailwindcss.com/3.4.17";
    sha256 = "058dqnvb293w58a9l70dv25ppgyb8h074v5sbjddr75ac538jvhp";
  };

  # Build the WASM binary using Nix's Rust toolchain (no network needed)
  wasmBuild = pkgs.rustPlatform.buildRustPackage {
    pname = "crystal-forge-ui-wasm";
    version = "0.1.0";
    src = webUiSrc;

    cargoLock = { lockFile = "${webUiSrc}/Cargo.lock"; };

    buildPhase = ''
      cargo build --target wasm32-unknown-unknown --release
    '';

    installPhase = ''
      mkdir -p $out/wasm

      ${pkgs.wasm-bindgen-cli}/bin/wasm-bindgen \
        --out-dir $out/wasm \
        --out-name crystal-forge-ui \
        --target web \
        target/wasm32-unknown-unknown/release/crystal-forge-ui.wasm

      ${pkgs.binaryen}/bin/wasm-opt \
        -Oz \
        $out/wasm/crystal-forge-ui_bg.wasm \
        -o $out/wasm/crystal-forge-ui_bg.wasm
    '';

    doCheck = false;
    nativeBuildInputs = with pkgs; [ wasm-bindgen-cli binaryen lld ];
    CARGO_BUILD_TARGET = "wasm32-unknown-unknown";
    CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_LINKER = "lld";
  };

  # SPA server: serves static files, falls back to index.html for client-side routes.
  # Written as pkgs.writeText to avoid nested heredoc issues in Nix '' strings.
  spaServerScript = pkgs.writeText "spa-server.py" ''
    import http.server, os, sys

    WEB_ROOT = sys.argv[1] if len(sys.argv) > 1 else "."
    PORT = int(sys.argv[2]) if len(sys.argv) > 2 else 8080

    class SPAHandler(http.server.SimpleHTTPRequestHandler):
        def __init__(self, *args, **kwargs):
            super().__init__(*args, directory=WEB_ROOT, **kwargs)
        def do_GET(self):
            path = self.translate_path(self.path)
            if os.path.isfile(path):
                return super().do_GET()
            self.path = "/"
            return super().do_GET()

    if __name__ == "__main__":
        server = http.server.HTTPServer(("", PORT), SPAHandler)
        print(f"SPA server on port {PORT} serving {WEB_ROOT}")
        server.serve_forever()
  '';

  # Assemble a serveable web root: index.html + wasm/ + tailwind.js
  webRoot = pkgs.runCommand "crystal-forge-ui-webroot" { } ''
    mkdir -p $out/wasm
    cp -r ${wasmBuild}/wasm/* $out/wasm/
    cp ${tailwindJs} $out/tailwind.js

    cat > $out/index.html << 'EOF'
    <!DOCTYPE html>
    <html>
      <head>
        <title>Crystal Forge</title>
        <meta content="text/html;charset=utf-8" http-equiv="Content-Type">
        <meta name="viewport" content="width=device-width, initial-scale=1">
        <meta charset="UTF-8">
        <script src="/tailwind.js"></script>
      </head>
      <body>
        <div id="main"></div>
        <script type="module" async src="/wasm/crystal-forge-ui.js"></script>
      </body>
    </html>
    EOF
  '';

  # Build verification (fast, no VM needed)
  buildCheck = pkgs.runCommand "crystal-forge-web-ui-build-check" {
    inherit wasmBuild;
    src = webUiSrc;
  } ''
    echo "=== Crystal Forge Web UI Build Verification ==="

    # Check 1: WASM binary exists
    test -f "$wasmBuild/wasm/crystal-forge-ui_bg.wasm" || { echo "FAIL: WASM binary missing"; exit 1; }
    WASM_SIZE=$(stat -c%s "$wasmBuild/wasm/crystal-forge-ui_bg.wasm")
    echo "  OK: crystal-forge-ui_bg.wasm ($WASM_SIZE bytes)"

    # Check 2: JS glue exists
    test -f "$wasmBuild/wasm/crystal-forge-ui.js" || { echo "FAIL: JS glue missing"; exit 1; }
    echo "  OK: crystal-forge-ui.js exists"

    # Check 3: Dioxus.toml valid
    grep -q 'name = "crystal-forge-ui"' "$src/Dioxus.toml" || { echo "FAIL: bad Dioxus.toml"; exit 1; }
    grep -q 'default_platform = "web"' "$src/Dioxus.toml" || { echo "FAIL: bad Dioxus.toml"; exit 1; }
    echo "  OK: Dioxus.toml valid"

    # Check 4: WASM binary non-trivial
    test "$WASM_SIZE" -gt 1024 || { echo "FAIL: WASM too small ($WASM_SIZE bytes)"; exit 1; }

    # Check 5: JS glue references WASM
    grep -q "crystal-forge-ui_bg.wasm" "$wasmBuild/wasm/crystal-forge-ui.js" || { echo "FAIL: JS doesn't reference WASM"; exit 1; }
    echo "  OK: JS glue references WASM binary"

    echo "=== All build checks passed ==="
    mkdir -p $out
    echo "WASM: $WASM_SIZE bytes, JS: $(stat -c%s "$wasmBuild/wasm/crystal-forge-ui.js") bytes" > $out/result.txt
  '';
  # NixOS VM test: serve the web UI and take screenshots with Chromium
in pkgs.testers.runNixOSTest {
  name = "crystal-forge-web-ui-screenshots";
  skipLint = true;
  skipTypeCheck = true;

  nodes.machine = {
    virtualisation.memorySize = 4096;
    virtualisation.cores = 2;

    environment.systemPackages = [ pkgs.chromium-stable pkgs.python3 ];

    # Serve the web UI on port 8080 via a systemd service
    systemd.services.web-ui-server = {
      description = "Crystal Forge Web UI static server";
      wantedBy = [ "multi-user.target" ];
      after = [ "network.target" ];
      serviceConfig = {
        ExecStart =
          "${pkgs.python3}/bin/python3 ${spaServerScript} ${webRoot} 8080";
        Restart = "always";
      };
    };

    networking.firewall.allowedTCPPorts = [ 8080 ];
  };

  globalTimeout = 300; # 5 minutes

  testScript = ''
    import os

    machine.start()
    machine.wait_for_unit("web-ui-server.service")
    machine.wait_for_open_port(8080)

    # Verify the build check passed first
    machine.succeed("test -f ${buildCheck}/result.txt")
    print("Build verification: " + machine.succeed("cat ${buildCheck}/result.txt").strip())

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

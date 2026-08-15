# Design example server package
#
# Serves docs/design/CrystalForge — the deterministic, fixture-backed design
# mock — over HTTP without requiring an ambient python3. Snowfall-lib
# auto-discovers this file as `packages.<system>.design-example-server`, so no
# flake.nix wiring is needed:
#
#   nix run .#design-example-server
#   nix run .#design-example-server -- --port 8091
#   nix run .#design-example-server -- --dir /path/to/live/checkout
#
# Defaults to port 8080 and the flake's own copy of the design example; pass
# --dir to point at a live checkout to see edits without re-running nix.
{ lib, pkgs, inputs, ... }:
let
  defaultDesignDir = "${inputs.self}/docs/design/CrystalForge";
in
pkgs.writeShellApplication {
  name = "cf-design-example-server";
  runtimeInputs = [ pkgs.python3 ];
  meta.mainProgram = "cf-design-example-server";
  text = ''
    PORT="8080"
    DESIGN_DIR="${defaultDesignDir}"

    usage() {
      echo "Usage: nix run .#design-example-server [--port <port>] [--dir <path>]"
      echo ""
      echo "  --port <port>  Port to listen on (default: 8080)."
      echo "  --dir <path>   Directory to serve (default: the flake's docs/design/CrystalForge)."
      echo "                 Point this at a live checkout to see edits without re-running nix."
      echo "  --help, -h     Show this help."
    }

    while [ "$#" -gt 0 ]; do
      case "$1" in
        --port)
          PORT="$2"; shift 2 ;;
        --dir)
          DESIGN_DIR="$2"; shift 2 ;;
        --help|-h)
          usage
          exit 0 ;;
        *)
          echo "ERROR: Unknown argument: $1" >&2
          usage >&2
          exit 2 ;;
      esac
    done

    if [ ! -f "$DESIGN_DIR/crystal-forge.html" ]; then
      echo "ERROR: crystal-forge.html not found in $DESIGN_DIR" >&2
      exit 1
    fi

    echo "────────────────────────────────────────────────────────────"
    echo "  Crystal Forge Design Example"
    echo "  Serving: $DESIGN_DIR"
    echo "  URL:     http://localhost:$PORT/crystal-forge.html?view=dashboard&theme=dark"
    echo ""
    echo "  Open the above URL in your browser to verify the design."
    echo "  Change ?view=dashboard to any view name from the manifest."
    echo "  Change &theme=dark to &theme=light for light mode."
    echo "────────────────────────────────────────────────────────────"

    cd "$DESIGN_DIR"
    exec python3 -m http.server "$PORT"
  '';
}
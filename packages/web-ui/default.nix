{ lib, pkgs, ... }:
let src = ./.;
in pkgs.writeShellApplication {
  name = "crystal-forge-ui";
  runtimeInputs = with pkgs; [
    dioxus-cli
    rustc
    cargo
    wasm-bindgen-cli
    binaryen
  ];
  text = ''
    set -euo pipefail

    # Resolve the source directory — use $PROJECT_ROOT if set (dev shell),
    # otherwise fall back to the Nix store copy of the source.
    SRC_DIR="''${PROJECT_ROOT:-}"
    if [ -n "$SRC_DIR" ] && [ -d "$SRC_DIR/packages/web-ui/src" ]; then
      SRC_DIR="$SRC_DIR/packages/web-ui"
    else
      # Copy source to a writable temp directory since dx needs to write to target/
      SRC_DIR="$(mktemp -d)"
      trap 'rm -rf "$SRC_DIR"' EXIT
      cp -r ${src}/. "$SRC_DIR/"
      chmod -R u+w "$SRC_DIR"
    fi

    echo "🌐 Starting Crystal Forge Web UI..."
    echo "   Source: $SRC_DIR"
    echo "   URL:    http://localhost:8080"
    echo ""

    cd "$SRC_DIR"
    dx serve "$@"
  '';
}

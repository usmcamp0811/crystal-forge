# Nix run app: generate design example screenshots
#
# Usage:
#   nix run .#generate-design-targets -- --out-dir ./design-shots
#   nix run .#generate-design-targets -- --out-dir ./design-shots --fixtures ./custom-fixtures.json
#
# Renders each view+theme from the design-parity manifest against the vendored
# offline design example (docs/design/CrystalForge) and saves
# <view>--<theme>.design.png screenshots to the output directory.
{ lib, pkgs, inputs, ... }:
let
  # ── Vendored offline design example ──────────────────────────────────────────
  reactUmd = pkgs.fetchurl {
    url = "https://unpkg.com/react@18.3.1/umd/react.development.js";
    sha256 = "0zsfq9pj3pbpiz9p6k6qflwd33s24kwflbdjxqn8pvdhdkpqyd18";
  };
  reactDomUmd = pkgs.fetchurl {
    url = "https://unpkg.com/react-dom@18.3.1/umd/react-dom.development.js";
    sha256 = "1r09hyz12n03w6fvcnv93ri0mv16wljgkpq4laqqpnrrkig4l17r";
  };
  babelStandalone = pkgs.fetchurl {
    url = "https://unpkg.com/@babel/standalone@7.29.0/babel.min.js";
    sha256 = "186f1mfjlcs49p0j0hss1m9cxpbpw9a12imli7kmr48953iaj8r6";
  };

  designExampleSrc = "${inputs.self}/docs/design/CrystalForge";

  designExampleOffline = pkgs.runCommand "cf-design-example-offline" { } ''
    mkdir -p $out/vendor
    cp -r ${designExampleSrc}/. $out/
    chmod -R u+w $out
    cp ${reactUmd} $out/vendor/react.development.js
    cp ${reactDomUmd} $out/vendor/react-dom.development.js
    cp ${babelStandalone} $out/vendor/babel.min.js
    ${pkgs.gnused}/bin/sed -i -E \
      -e 's#src="https://unpkg.com/react@[^"]*"#src="vendor/react.development.js"#' \
      -e 's#src="https://unpkg.com/react-dom@[^"]*"#src="vendor/react-dom.development.js"#' \
      -e 's#src="https://unpkg.com/@babel/standalone@[^"]*"#src="vendor/babel.min.js"#' \
      -e 's# integrity="[^"]*"##g' \
      -e 's# crossorigin="anonymous"##g' \
      $out/crystal-forge.html
  '';

  parityDir = "${inputs.self}/checks/web-ui/design-parity";

  script = pkgs.writeShellApplication {
    name = "cf-generate-design-targets";
    runtimeInputs = [
      pkgs.nodejs
      pkgs.playwright-test
      pkgs.playwright-driver
      pkgs.imagemagick
    ];
    text = ''
      OUT_DIR=""
      FIXTURES=""

      while [ $# -gt 0 ]; do
        case "$1" in
          --out-dir)
            OUT_DIR="$2"; shift 2 ;;
          --fixtures)
            FIXTURES="$2"; shift 2 ;;
          --help|-h)
            echo "Usage: nix run .#generate-design-targets -- --out-dir <dir> [--fixtures <json>]"
            echo ""
            echo "  --out-dir <dir>   Where <view>--<theme>.design.png files are written (required)."
            echo "  --fixtures <json> Path to a custom fixtures JSON to overlay on the design example."
            exit 0 ;;
          *)
            echo "ERROR: Unknown argument: $1"
            exit 2 ;;
        esac
      done

      if [ -z "$OUT_DIR" ]; then
        echo "ERROR: --out-dir is required"
        echo "Usage: nix run .#generate-design-targets -- --out-dir <dir> [--fixtures <json>]"
        exit 2
      fi

      mkdir -p "$OUT_DIR"

      # Set up Playwright to use the Nix-provided chromium
      export PLAYWRIGHT_BROWSERS_PATH="${pkgs.playwright-driver.browsers}"
      export NODE_PATH="${pkgs.playwright-test}/lib/node_modules"

      DESIGN_DIR="${designExampleOffline}"

      # If a custom fixtures JSON is provided, overlay it on the design example
      # (the design example reads fixtures from fixtures/crystal-forge.fixtures.json)
      if [ -n "$FIXTURES" ]; then
        if [ -f "$FIXTURES" ]; then
          cp "$FIXTURES" "$DESIGN_DIR/fixtures/crystal-forge.fixtures.json"
          echo "Using custom fixtures: $FIXTURES"
        else
          echo "WARNING: fixtures file not found: $FIXTURES — using default"
        fi
      fi

      echo "Generating design target screenshots..."
      echo "  Design dir: $DESIGN_DIR"
      echo "  Output dir: $OUT_DIR"
      echo ""

      node "${parityDir}/generate-design-targets.js" \
        "$DESIGN_DIR" \
        "${parityDir}/manifest.json" \
        "$OUT_DIR"

      echo ""
      echo "Done. Screenshots saved to: $OUT_DIR"
    '';
  };
in {
  type = "app";
  program = "${script}/bin/cf-generate-design-targets";
}

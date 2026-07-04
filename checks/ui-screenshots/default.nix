# UI Screenshot Check
#
# Lightweight, backend-free visual check for the Dioxus web UI.
# Serves the pre-built WASM bundle, intercepts all /api/v1/ calls with
# fixture JSON, and screenshots 13 views × 2 themes = 26 PNGs.
#
# No Crystal Forge server, no database, no network needed.
#
# Build:
#   nix build .#checks.x86_64-linux.ui-screenshots
#   nix build .#ui-screenshots
#   ls result/
#
{ lib, pkgs, inputs, ... }:

let
  fixturesPath = "${inputs.self}/docs/design/CrystalForge/fixtures/crystal-forge.fixtures.json";
  captureScript = "${inputs.self}/checks/ui-screenshots/capture.js";
  # Use the fixture-mode build: same WASM but compiled with --features ui_fixture_mode
  # so that ?ui_check_auth=1 bypasses the login screen in release mode.
  webUiPublic   = "${pkgs.crystal-forge.web-ui.web-app-fixture}/public";

in pkgs.runCommand "crystal-forge-ui-screenshots"
  {
    __noChroot        = true;
    nativeBuildInputs = [ pkgs.nodejs pkgs.playwright-test ];
    PLAYWRIGHT_BROWSERS_PATH = "${pkgs.playwright-driver.browsers}";
    NODE_PATH                = "${pkgs.playwright-test}/lib/node_modules";
  }
  ''
    mkdir -p $out

    node ${captureScript} \
      ${webUiPublic} \
      ${fixturesPath} \
      $out
  ''

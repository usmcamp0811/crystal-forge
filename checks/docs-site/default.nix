# Standalone static-site browser check. No VM or application services are needed.
{ lib, pkgs, inputs, ... }:
let
  python = pkgs.python3.withPackages (p: [ p.playwright ]);
  site = pkgs.crystal-forge.docs-site or (import ../../packages/docs-site {
    inherit lib pkgs inputs;
  });
in
pkgs.runCommand "crystal-forge-docs-site-browser" {
  nativeBuildInputs = [ python ];
  PLAYWRIGHT_BROWSERS_PATH = "${pkgs.playwright-driver.browsers}";
  PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD = "1";
  # Native Chromium needs a real fontconfig setup inside the Nix sandbox.
  FONTCONFIG_FILE = pkgs.makeFontsConf { fontDirectories = [ pkgs.dejavu_fonts ]; };
  meta.description = "Check static documentation at root and deployment-prefix URLs, including every Mermaid diagram";
} ''
  export HOME="$TMPDIR/home"
  export XDG_CACHE_HOME="$TMPDIR/cache"
  export PYTHONDONTWRITEBYTECODE=1
  mkdir -p "$HOME" "$XDG_CACHE_HOME" "$out/screenshots"
  ${python}/bin/python ${./run.py} \
    --site ${site}/share/docs-site \
    --browser-test ${../../packages/docs-site/browser_test.py} \
    --screenshots "$out/screenshots"
''

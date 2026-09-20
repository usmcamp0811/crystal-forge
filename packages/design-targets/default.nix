# Design target screenshots package
#
# Runs Playwright + Chromium headless inside a plain Nix sandbox (no VM).
# All dependencies are pinned store paths; result is fully cached.
#
#   nix build .#design-targets   → ./result/ has <view>--<theme>.design.png
#
{ lib, pkgs, inputs, ... }:
let
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
  jszip = pkgs.fetchurl {
    url = "https://unpkg.com/jszip@3.10.1/dist/jszip.min.js";
    hash = "sha256-rMfkFFWoB2W1/Zx+4bgHim0WC7vKRVrq6FTeZclH1Z4=";
  };

  designExampleSrc = "${inputs.self}/docs/design/CrystalForge";

  designExampleOffline = pkgs.runCommand "cf-design-example-offline" { } ''
    mkdir -p $out/vendor
    cp -r ${designExampleSrc}/. $out/
    chmod -R u+w $out
    cp ${reactUmd}        $out/vendor/react.development.js
    cp ${reactDomUmd}     $out/vendor/react-dom.development.js
    cp ${babelStandalone} $out/vendor/babel.min.js
    cp ${jszip}           $out/vendor/jszip.min.js
    ${pkgs.gnused}/bin/sed -i -E \
      -e 's#src="https://unpkg.com/react@[^"]*"#src="vendor/react.development.js"#' \
      -e 's#src="https://unpkg.com/react-dom@[^"]*"#src="vendor/react-dom.development.js"#' \
      -e 's#src="https://unpkg.com/@babel/standalone@[^"]*"#src="vendor/babel.min.js"#' \
      -e 's#src="https://unpkg.com/jszip@[^"]*"#src="vendor/jszip.min.js"#' \
      -e 's# integrity="[^"]*"##g' \
      -e 's# crossorigin="anonymous"##g' \
      $out/crystal-forge.html
  '';

  parityDir = "${inputs.self}/checks/web-ui/design-parity";

# __noChroot = true: Chromium needs /proc and clone(2) which the Nix sandbox
# blocks. All inputs are still pinned store paths so the result is
# deterministic even without the chroot. Requires sandbox = relaxed or false
# in nix.conf (this repo's NixOS config already sets sandbox = relaxed).
in pkgs.runCommand "crystal-forge-design-targets"
  {
    __noChroot               = true;
    nativeBuildInputs        = [ pkgs.nodejs pkgs.playwright-test ];
    PLAYWRIGHT_BROWSERS_PATH = "${pkgs.playwright-driver.browsers}";
    NODE_PATH                = "${pkgs.playwright-test}/lib/node_modules";
  }
  ''
    mkdir -p $out

    node ${parityDir}/generate-design-targets.js \
      ${designExampleOffline} \
      ${parityDir}/manifest.json \
      $out
  ''

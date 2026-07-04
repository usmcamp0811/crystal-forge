# Design target screenshots package
#
# Builds the design example screenshots (one per view × theme) inside a
# minimal NixOS VM and writes them to the Nix store so the result is
# reproducible and cacheable.
#
# Build:
#   nix build .#design-targets
#   ls result/
#
# Run (opens the output directory):
#   nix run .#design-targets
#
# Custom fixtures:
#   nix build .#design-targets --override-input fixtures path:/path/to/custom-fixtures.json
#   (or pass fixture path at build time via the fixturesFile argument)
{ lib, pkgs, inputs, ... }:
let
  # ── Vendored offline design example (same as web-ui check) ──────────────────
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
    cp ${reactUmd}      $out/vendor/react.development.js
    cp ${reactDomUmd}   $out/vendor/react-dom.development.js
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

in pkgs.testers.runNixOSTest {
  name = "crystal-forge-design-targets";

  nodes.machine = {
    virtualisation.memorySize = 2048;
    virtualisation.cores = 2;

    environment.systemPackages = [
      pkgs.chromium
      pkgs.nodejs
      pkgs.playwright-test
      pkgs.imagemagick
    ];

    environment.variables = {
      NODE_PATH               = "${pkgs.playwright-test}/lib/node_modules";
      PLAYWRIGHT_BROWSERS_PATH = "${pkgs.playwright-driver.browsers}";
    };
  };

  testScript = ''
    machine.wait_for_unit("multi-user.target")

    machine.succeed("mkdir -p /tmp/design-example /tmp/design-parity /tmp/out")
    machine.succeed("cp -r ${designExampleOffline}/. /tmp/design-example/")
    machine.succeed("cp -r ${parityDir}/. /tmp/design-parity/")

    print("Generating design target screenshots...")
    machine.succeed(
      "${pkgs.nodejs}/bin/node /tmp/design-parity/generate-design-targets.js "
      "/tmp/design-example "
      "/tmp/design-parity/manifest.json "
      "/tmp/out "
      "2>&1"
    )

    machine.copy_from_vm("/tmp/out", "")
  '';
}

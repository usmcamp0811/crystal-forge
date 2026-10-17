# Pinned browser and render profile shared by native integration checks.
#
# The browser runs inside the VM that hosts it. All versions come from the
# repository's locked `nixpkgs` input, so a browser, font, or Playwright change
# is a rendering-environment change and is reviewed as one.
#
# SECURITY: Crystal Forge sets `__Host-` cookies with the `Secure` attribute.
# Chromium accepts them only from a secure context. The supported placement is
# a browser on the controller that opens `http://localhost:<port>`; Chromium
# treats loopback origins as secure. A cross-host browser needs a TLS origin.
{
  pkgs,
  lib,
  inputs,
}: let
  facts = import ./facts.nix {inherit pkgs lib inputs;};

  # The single canonical render profile. Add a viewport only for a distinct
  # supported layout or behavior (contract section 6).
  profile = {
    name = "cf-it-render-v1";
    playwright = pkgs.playwright-test.version;
    browsers = "${pkgs.playwright-driver.browsers}";
    locale = "en-US";
    timezoneId = "UTC";
    colorScheme = "dark";
    reducedMotion = "reduce";
    deviceScaleFactor = 1;
    fonts = {
      sansSerif = "DejaVu Sans";
      serif = "DejaVu Serif";
      monospace = "JetBrains Mono";
    };
    viewports = {
      desktop = {
        width = 1440;
        height = 900;
      };
      narrow = {
        width = 375;
        height = 812;
      };
    };
    # Bounded default for every browser assertion and wait, in milliseconds.
    # A longer wait MUST be passed explicitly at the call site with a reason.
    defaultTimeoutMs = 15000;
  };

  profileFile = pkgs.writeText "cf-it-render-profile.json" (builtins.toJSON profile);

  cfBrowser = pkgs.writeShellApplication {
    name = "cf-browser";
    runtimeInputs = [pkgs.nodejs pkgs.imagemagick];
    text = ''
      export NODE_PATH="${pkgs.playwright-test}/lib/node_modules"
      export PLAYWRIGHT_BROWSERS_PATH="${pkgs.playwright-driver.browsers}"
      export CF_IT_RENDER_PROFILE="${profileFile}"
      export CF_IT_DESIGN_SITE="${facts.designSite}"
      exec node ${./browser/cf-browser.cjs} "$@"
    '';
  };

  # NixOS module that adds the pinned browser to a role.
  module = {...}: {
    fonts.enableDefaultPackages = false;
    fonts.packages = [pkgs.dejavu_fonts pkgs.jetbrains-mono];
    fonts.fontconfig.defaultFonts = {
      sansSerif = [profile.fonts.sansSerif];
      serif = [profile.fonts.serif];
      monospace = [profile.fonts.monospace];
    };
    environment.systemPackages = [cfBrowser];
    environment.etc."cf-it/render-profile.json".source = profileFile;
  };
in {
  inherit profile profileFile cfBrowser module;
}

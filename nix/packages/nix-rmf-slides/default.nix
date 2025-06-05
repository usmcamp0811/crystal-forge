{
  pkgs,
  lib,
  inputs,
  ...
}:
with lib;
with lib.campground; let
  slides = mkSlide {
    inherit lib;
    stdenv = pkgs.stdenv;
    slidev = pkgs.campground.slidev.v0_50_0;
    markdown = ./slides.md;
    themes = [
      # pkgs.campground.slidev-themes
      # pkgs.campground.slidev-themes.neversink-theme
      pkgs.campground.slidev-themes.mokkapps-theme
      # pkgs.campground.slidev-themes.csscade-theme
    ];
    slides = [./slides];
    assets = [./assets];
    # extraNodePackages = [pkgs.campground.sass-embedded];
  };

  serve = pkgs.writeShellApplication {
    name = "serve";
    text = ''
      ${pkgs.python3}/bin/python3 -m http.server 8080 --directory ${slides}
    '';
  };

  serve-dev = pkgs.writeShellApplication {
    name = "serve-dev";
    runtimeInputs = [pkgs.coreutils];
    text = ''
      if [ ! -f slides.md ]; then
        echo "Error: slides.md not found in the current directory."
        exit 1
      fi

      VITE_CACHE_DIR=$(mktemp -d)
      export VITE_CACHE_DIR

      # Define cleanup
      cleanup() {
        rm -rf "$VITE_CACHE_DIR"
        rm -rf themes
        rm -f ./pnpm-lock.yaml
        rm -f node_modules/@slidev/theme-default
        rmdir node_modules/.pnpm 2>/dev/null || true
        rmdir node_modules/@slidev 2>/dev/null || true
        rmdir node_modules/prism-theme-vars 2>/dev/null || true
        rmdir node_modules 2>/dev/null || true
        rm -rf .vite
      }
      trap cleanup EXIT

      # Setup
      rm -rf themes
      cp -r --no-preserve=mode,ownership ${slides}/themes themes
      mkdir -p node_modules/.pnpm
      mkdir -p node_modules/@slidev
      mkdir -p node_modules/prism-theme-vars
      touch pnpm-lock.yaml

      ${pkgs.campground.slidev.v0_50_0}/bin/slidev --remote
    '';
  };
in
  slides
  // {
    inherit serve;
    dev = serve-dev;
  }

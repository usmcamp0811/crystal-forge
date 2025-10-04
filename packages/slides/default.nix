{
  pkgs,
  lib,
  inputs,
  ...
}:
with lib;
with lib.campground; let
  slidev = pkgs.campground.slidev.v0_50_0;
  stdenv = pkgs.stdenv;

  slides = mkSlide {
    inherit lib stdenv slidev;
    markdown = ./slides.md;
    urlBase = "/devsecops-revolution/";
    themes = [pkgs.campground.slidev-themes.neversink-theme];
    slides = [./slides];
    assets = [./assets];
    meta = {title = "A Nix Powered DevSecOps Revolution";};
  };

  allSlides = {
    mac-builder = mac-builder;
    beyond-yaml = beyond-yaml;
    devsecops-revolution = slides;
  };

  index-page = makeIndexPage {
    inherit pkgs;
    slides = allSlides;
  };

  index-site =
    pkgs.runCommand "slide-index"
    {
      buildInputs = [pkgs.coreutils];
    } ''
      mkdir -p $out
      ${builtins.concatStringsSep "\n" (
        builtins.map (name: "cp -r ${getAttr name allSlides} $out/${name}") (builtins.attrNames allSlides)
      )}
      cp ${index-page} $out/index.html
    '';

  serve-index = pkgs.writeShellApplication {
    name = "serve-index";
    runtimeInputs = [pkgs.python3];
    text = ''
      PORT="''${1:-8000}"
      cd ${index-site}
      echo "Serving on http://localhost:$PORT"
      ${pkgs.python3}/bin/python -m http.server "$PORT"
    '';
  };

  serve-dev = pkgs.writeShellApplication {
    name = "serve-dev";
    runtimeInputs = [pkgs.coreutils];
    text = ''
      SLIDE_FILE="''${1:-slides.md}"
      [ -z "$SLIDE_FILE" ] && SLIDE_FILE="slides.md"

      if [ ! -f "$SLIDE_FILE" ]; then
        echo "Error: $SLIDE_FILE not found in the current directory."
        exit 1
      fi

      VITE_CACHE_DIR=$(mktemp -d)
      export VITE_CACHE_DIR

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

      rm -rf themes
      cp -r --no-preserve=mode,ownership ${slides}/themes themes
      mkdir -p node_modules/.pnpm
      mkdir -p node_modules/@slidev
      mkdir -p node_modules/prism-theme-vars
      touch pnpm-lock.yaml

      ${pkgs.campground.slidev.v0_50_0}/bin/slidev "$SLIDE_FILE" --remote
    '';
  };
in
  index-site
  // {
    inherit mac-builder beyond-yaml slides serve-index;
    dev = serve-dev;
  }

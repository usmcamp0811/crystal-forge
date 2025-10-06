{
  pkgs,
  lib,
  inputs,
  stdenv,
  ...
}:
with lib;
with lib.crystal-forge; let
  slides = mkSlide {
    inherit pkgs lib stdenv;
    markdown = ./slides.md;
    slides = [./slides];
    assets = [./assets];
    customCss = ./style.css;
    meta = {title = "A Nix Powered DevSecOps Revolution";};
  };
in
  slides
  // {
    server = pkgs.writeShellScriptBin "serve-slides" ''
      PORT=''${PORT:-3044}
      echo "Starting slide server on http://localhost:$PORT"
      ${pkgs.python3}/bin/python -m http.server $PORT --directory ${slides}
    '';
  }

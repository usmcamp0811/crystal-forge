{
  inputs,
  pkgs,
  lib,
  ...
}: let
  lib = pkgs.lib;

  # Create a git repository from the current flake source
  # Since this file is in checks/build-flake/, the flake root is ../..
  crystalForgeFlakeGit = pkgs.stdenv.mkDerivation {
    name = "flake-as-git";

    # Copy the current directory (but be careful about .git)
    src = ../../.;

    nativeBuildInputs = with pkgs; [git];

    buildPhase = ''
      # Create output directory
      mkdir -p $out

      # Copy all files from source
      cp -r . $out/

      # Initialize git repo in the output
      cd $out
      git init
      git config user.name "Nix Build"
      git config user.email "nix@build.local"

      # Add all files and commit
      git add .
      git commit -m "Packaged flake for testing"
    '';

    # No install phase needed since we build directly into $out
    dontInstall = true;
  };
in
  crystalForgeFlakeGit

{
  lib,
  inputs,
  system ? null,
  ...
}: let
  # Default to x86_64-linux if no system specified
  actualSystem =
    if system != null
    then system
    else "x86_64-linux";

  pkgs = inputs.nixpkgs.legacyPackages.${actualSystem};

  # Create a minimal, static flake that doesn't change
  staticFlakeContent = pkgs.writeTextFile {
    name = "minimal-flake";
    destination = "/flake.nix";
    text = ''
      {
        inputs = {
          nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
        };
        outputs = { self, nixpkgs }: let
          system = "x86_64-linux";
          pkgs = nixpkgs.legacyPackages.${system};
        in {
          packages.${system}.default = pkgs.hello;

          nixosConfigurations.cf-test-sys = nixpkgs.lib.nixosSystem {
            inherit system;
            modules = [{
              boot.isContainer = true;
              fileSystems."/" = { device = "none"; fsType = "tmpfs"; };
              services.getty.autologinUser = "root";
              system.stateVersion = "25.05";
              # Minimal config to avoid build complexity
              services.udisks2.enable = false;
              security.polkit.enable = false;
              documentation.enable = false;
              documentation.nixos.enable = false;
              system.nssModules = pkgs.lib.mkForce [];
            }];
          };
        };
      }
    '';
  };

  # Create flake.lock separately (also static)
  staticFlakeLock = pkgs.writeTextFile {
    name = "minimal-flake-lock";
    destination = "/flake.lock";
    text = builtins.toJSON {
      nodes = {
        nixpkgs = {
          inputs = {};
          originalRef = {
            owner = "NixOS";
            repo = "nixpkgs";
            type = "github";
          };
          locked = {
            owner = "NixOS";
            repo = "nixpkgs";
            rev = "abc123"; # Use a fixed rev to avoid changes
            type = "github";
            narHash = "sha256-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=";
          };
        };
        root = {
          inputs.nixpkgs = "nixpkgs";
        };
      };
      root = "root";
      version = 7;
    };
  };

  # Combine static content
  staticFlake = pkgs.runCommand "static-flake" {} ''
    mkdir -p $out
    cp ${staticFlakeContent}/flake.nix $out/
    cp ${staticFlakeLock}/flake.lock $out/
  '';

  # Create git repo with deterministic timestamps (this part is cached)
  testFlakeOptimized =
    pkgs.runCommand "crystal-forge-test-flake.git" {
      nativeBuildInputs = [pkgs.git];
      # Make this derivation more cacheable by not depending on build time
      preferLocalBuild = true;
    } ''
      set -eu
      export HOME=$PWD
      work="$TMPDIR/work"

      mkdir -p "$work"
      cp -r ${staticFlake}/* "$work/"
      cd "$work"
      chmod -R u+w .

      git init -q
      git config user.name "Crystal Forge Test"
      git config user.email "test@crystal-forge.dev"

      # Use deterministic timestamps instead of $(date)
      export GIT_AUTHOR_DATE="2024-01-01T00:00:00Z"
      export GIT_COMMITTER_DATE="2024-01-01T00:00:00Z"

      git add -f flake.nix flake.lock
      git commit -q -m "Initial flake configuration"

      export GIT_AUTHOR_DATE="2024-01-02T00:00:00Z"
      export GIT_COMMITTER_DATE="2024-01-02T00:00:00Z"
      echo "# Crystal Forge Test Environment" >> flake.nix
      git add -f flake.nix
      git commit -q -m "Add documentation comment"

      export GIT_AUTHOR_DATE="2024-01-03T00:00:00Z"
      export GIT_COMMITTER_DATE="2024-01-03T00:00:00Z"
      echo "# Version: 1.0.0" >> flake.nix
      git add -f flake.nix
      git commit -q -m "Add version info"

      export GIT_AUTHOR_DATE="2024-01-04T00:00:00Z"
      export GIT_COMMITTER_DATE="2024-01-04T00:00:00Z"
      echo "# Last updated: 2024-01-04" >> flake.nix
      git add -f flake.nix
      git commit -q -m "Update timestamp"

      # Create bare repository
      git init --bare "$out"
      git -C "$out" config receive.denyCurrentBranch ignore
      git push "$out" HEAD:refs/heads/main
      git -C "$out" symbolic-ref HEAD refs/heads/main

      git -C "$out" config http.receivepack true
      git -C "$out" config http.uploadpack true
      git -C "$out" update-server-info
    '';
in {
  inherit testFlakeOptimized staticFlake;
}

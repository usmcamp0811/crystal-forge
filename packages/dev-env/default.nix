{
  lib,
  pkgs,
  inputs,
  ...
}: let
  # Import nixos-compose from inputs
  nxc = inputs.nixos-compose;
in
  nxc.lib.compose {
    inherit (pkgs) system;
    nixpkgs = inputs.nixpkgs;
    composition = ./composition.nix;

    # Optional: Add any extra configurations if needed
    extraConfigurations = [
      # Add crystal-forge module
      inputs.self.nixosModules.crystal-forge
    ];
  }

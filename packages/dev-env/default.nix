{
  lib,
  pkgs,
  system,
  inputs,
  ...
}:
pkgs.nxc.compose {
  inherit system;
  nixpkgs = pkgs;
  composition = ./composition.nix;

  # Optional: Add any extra configurations if needed
  extraConfigurations = [
    # Add crystal-forge module
    inputs.self.nixosModules.crystal-forge
  ];
}

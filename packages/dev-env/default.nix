{
  lib,
  pkgs,
  system,
  inputs,
  ...
}: let
  # Use nixos-compose to build the composition
  composed = inputs.nixos-compose.lib.compose {
    inherit system;
    nixpkgs = inputs.nixpkgs;
    composition = ./composition.nix;
    extraConfigurations = [
      inputs.self.nixosModules.crystal-forge
    ];
  };

  # Get the composition info JSON
  vmComposition = composed."composition::vm";
in
  # Create a wrapper script that uses nxc to start the VM
  pkgs.writeShellScriptBin "dev-env-vm" ''
    set -euo pipefail

    echo "Starting Crystal Forge development VM using nixos-compose..."
    echo "Composition info: ${vmComposition}"
    echo ""

    # Use nxc start with the composition info
    exec ${pkgs.nxc}/bin/nxc start \
      --compose-info "${vmComposition}" \
      --interactive \
      "$@"
  ''
  // {inherit composed;}

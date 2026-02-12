{ channels, process-compose-flake, nixos-compose, ... }:
final: prev: {
  process-compose-flake = import process-compose-flake.lib { pkgs = final; };
  nxc-lib = nixos-compose.lib;
  nxc = nixos-compose.packages.${prev.system}.nixos-compose;

  # Pre-built Chromium from stable nixpkgs (always in binary cache)
  chromium-stable = stablePkgs.chromium;
}


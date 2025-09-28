{
  channels,
  process-compose-flake,
  nixos-compose,
  ...
}: final: prev: {
  process-compose-flake = import process-compose-flake.lib {pkgs = final;};
  nxc-lib = nixos-compose.lib;
  nxc = nixos-compose.packages.${prev.system}.nixos-compose;
}

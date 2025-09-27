{
  channels,
  process-compose-flake,
  nixos-compose,
  ...
}: final: prev: {
  process-compose-flake = import process-compose-flake.lib {pkgs = final;};
  nxc = nixos-compose.lib;
}

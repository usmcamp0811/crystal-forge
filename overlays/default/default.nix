{
  channels,
  process-compose-flake,
  ...
}: final: prev: {
  process-compose-flake = import process-compose-flake.lib {pkgs = final;};
}

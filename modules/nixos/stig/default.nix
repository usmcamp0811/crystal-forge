{
  lib,
  inputs,
  ...
}: {
  imports = lib.snowfall.fs.get-default-nix-files-recursive ./.;
}

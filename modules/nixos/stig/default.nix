{
  lib,
  inputs,
  ...
}: {
  imports = inputs.snowfall-lib.outputs.snowfall.internal-lib.fs.get-default-nix-files-recursive ./.;
}

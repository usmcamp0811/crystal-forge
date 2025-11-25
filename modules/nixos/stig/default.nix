{
  lib,
  inputs,
  ...
}: {
  imports = inputs.snowfall.lib.modules.importDir ./.;
}

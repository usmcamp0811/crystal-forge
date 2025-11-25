{
  lib,
  inputs,
  ...
}: {
  imports = inputs.snowfall-lib.lib.modules.importDir ./.;
}

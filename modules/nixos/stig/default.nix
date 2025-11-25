{lib, ...}: let
  fs = lib.snowfall.fs;

  # All default.nix files under this directory
  allDefaults = fs.get-default-nix-files-recursive ./.;

  # Drop this file itself so we don't self-import
  submoduleDefaults =
    builtins.filter (path: path != ./default.nix) allDefaults;
in {
  imports = submoduleDefaults;
}

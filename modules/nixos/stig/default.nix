{lib, ...}: let
  importDir = dir:
    builtins.map (name: dir + "/${name}")
    (builtins.filter (
      name:
        builtins.pathExists (dir + "/${name}/default.nix")
    ) (builtins.attrNames (builtins.readDir dir)));
in {
  imports = importDir ./.;
}

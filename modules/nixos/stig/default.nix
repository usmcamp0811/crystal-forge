{lib, ...}: let
  importDir = dir: let
    entries = builtins.readDir dir;
    paths =
      builtins.mapAttrs (
        name: type:
          if builtins.pathExists (dir + "/${name}/default.nix")
          then [(dir + "/${name}")]
          else if type == "directory"
          then importDir (dir + "/${name}")
          else []
      )
      entries;
  in
    builtins.concatLists (builtins.attrValues paths);
in {
  imports = importDir ../stig-modules;
}

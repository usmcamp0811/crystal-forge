{lib, ...}: {
  imports =
    builtins.filter
    (p: p != ./default.nix)
    (lib.snowfall.fs.get-default-nix-files-recursive ./.);
}

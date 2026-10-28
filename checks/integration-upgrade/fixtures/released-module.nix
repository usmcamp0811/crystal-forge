# Load the release's unchanged module with its own library and runtime packages.
# The stable key permits a normal NixOS specialisation to replace the module.
{predecessor}: args@{pkgs, ...}: {
  key = "upgrade-released-module";
  imports = [(import (predecessor + "/modules/nixos/crystal-forge")
    (args // {
      lib = predecessor.inputs.nixpkgs.lib;
      pkgs = predecessor.pkgs.${args.pkgs.stdenv.hostPlatform.system}.nixpkgs;
    }))];
}

{
  inputs.nixpkgs.url = "path:/nix/store/8gs4wzj7sc6is1sc2qffhzp3mpc5r7br-source";

  outputs = { nixpkgs, ... }:
    let
      lib = nixpkgs.lib;
      duplicateOptionShape = (lib.evalModules {
        modules = [
          ({ lib, ... }: {
            options.unrelated.duplicate = lib.mkOption {
              type = lib.types.bool;
              default = false;
            };
          })
          ({ lib, ... }: {
            options.unrelated.duplicate = lib.mkOption {
              type = lib.types.bool;
              default = false;
            };
          })
        ];
      }).options;
    in {
      nixosConfigurations.good = lib.evalModules {
        modules = [
          ({ lib, pkgs, ... }: {
            options.crystalForgeInspector = {
              boolean = lib.mkOption { type = lib.types.bool; default = true; };
              string = lib.mkOption { type = lib.types.str; default = "inspector"; };
              integer = lib.mkOption { type = lib.types.int; default = 42; };
              list = lib.mkOption { type = lib.types.listOf lib.types.str; default = [ "one" "two" ]; };
              attrs = lib.mkOption { type = lib.types.attrs; default = { key = "value"; }; };
              package = lib.mkOption { type = lib.types.package; default = pkgs.hello; };
              overridden = lib.mkOption { type = lib.types.str; default = "default"; };
            };
            config.crystalForgeInspector.overridden = lib.mkOverride 50 "winner";
          })
          ({ ... }: {
            config.crystalForgeInspector.overridden = "default";
          })
        ];
        specialArgs = { pkgs = nixpkgs.legacyPackages.x86_64-linux; };
      };

      # These values are deliberately lazy. Inspecting `good` must not force
      # either the broken configuration or the exported module.
      nixosConfigurations.unrelatedBroken = builtins.abort "unrelated configuration forced";
      nixosModules.unrelatedBroken = { lib, ... }:
        with lib.namespace-change-me;
        { namespace-change-me.enable = true; };
      checks.unrelatedDuplicateShape = duplicateOptionShape;
    };
}

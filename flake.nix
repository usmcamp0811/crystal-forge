{
  description = "Simple flake exporting a Rust package";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/release-nixos-25.05";
    flake-utils.url = "github:numtide/flake-utils";
    campground.url = "gitlab:usmcamp0811/dotfiles";
    snowfall-lib = {
      url = "github:snowfallorg/lib";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = inputs: let
    inherit (inputs) deploy-rs;

    lib = inputs.snowfall-lib.mkLib {
      inherit inputs;
      src = ./.;
      snowfall = {
        meta = {
          name = "crystal-froge";
          title = "Crystal Forge";
        };
        namespace = "crystal-froge";
      };
    };
  in
    lib.mkFlake {
      channels-config = {
        allowUnfree = true;
      };

      overlays = with inputs; [
        campground.overlays.default
      ];
    };
}

{
  description = "Simple flake exporting a Rust package";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    process-compose-flake.url = "github:Platonic-Systems/process-compose-flake";
    services-flake.url = "github:juspay/services-flake";
    snowfall-lib = {
      url = "github:snowfallorg/lib";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    nixos-compose = {
      url = "github:oar-team/nixos-compose/25.05";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = inputs: let
    lib = inputs.snowfall-lib.mkLib {
      inherit inputs;
      src = ./.;
      snowfall = {
        meta = {
          name = "crystal-forge";
          title = "Crystal Forge";
        };
        namespace = "crystal-forge";
      };
    };
  in
    lib.mkFlake {
      channels-config = {
        allowUnfree = true;
      };

      outputs-builder = channels: {
        packages = {
          agent = channels.nixpkgs.crystal-forge.default.agent;
          server = channels.nixpkgs.crystal-forge.default.server;
          builder = channels.nixpkgs.crystal-forge.default.builder;
          cf-keygen = channels.nixpkgs.crystal-forge.default.cf-keygen;
          test-agent = channels.nixpkgs.crystal-forge.default.test-agent;
        };
      };
    };
}

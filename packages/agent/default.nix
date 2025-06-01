{
  lib,
  pkgs,
  ...
}: let
  crystal-forge-agent = pkgs.rustPlatform.buildRustPackage {
    pname = "crystal-forge-agent";
    version = "0.1.0";

    src = ./.;

    cargoLock = {lockFile = ./Cargo.lock;};
    nativeBuildInputs = with pkgs; [pkg-config];
    buildInputs = with pkgs; [openssl];
  };
in
  crystal-forge-agent

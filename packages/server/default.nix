{
  lib,
  pkgs,
  ...
}: let
  crystal-forge-server = pkgs.rustPlatform.buildRustPackage {
    pname = "server";
    version = "0.1.0";

    src = ../../.;
    cargoLock = {
      lockFile = ../../Cargo.lock;
    };
    nativeBuildInputs = with pkgs; [pkg-config];
    buildInputs = [
      pkgs.rustc
      pkgs.cargo
      pkgs.pkg-config
      pkgs.openssl
    ];
  };
in
  crystal-forge-server

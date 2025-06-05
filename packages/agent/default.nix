{
  lib,
  pkgs,
  ...
}: let
  crystal-forge-agent = pkgs.rustPlatform.buildRustPackage {
    inherit src;
    pname = "agent";
    version = "0.1.0";

    cargoLock = {
      lockFile = "${src}/Cargo.lock";
    };
    nativeBuildInputs = with pkgs; [pkg-config];
    buildInputs = [
      pkgs.rustc
      pkgs.cargo
      pkgs.pkg-config
      pkgs.openssl
    ];
    # installPhase = ''
    #   install -Dm755 target/release/agent $out/bin/agent
    # '';
  };
in
  src

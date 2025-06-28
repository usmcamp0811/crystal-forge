{
  lib,
  pkgs,
  inputs,
  ...
}: let
  src = ./.;
  srcHash = builtins.hashString "sha256" (toString src);

  crystal-forge = pkgs.rustPlatform.buildRustPackage rec {
    inherit src;
    pname = "crystal-forge";
    version = "0.1.0";
    cargoLock = {
      lockFile = ./Cargo.lock;
    };
    nativeBuildInputs = with pkgs; [pkg-config];
    buildInputs = [
      pkgs.rustc
      pkgs.cargo
      pkgs.pkg-config
      pkgs.openssl
      pkgs.sqlx-cli
    ];

    # Set the GIT_HASH environment variable during build
    preBuild = ''
      export SRC_HASH="${lib.strings.removeSuffix "\n" srcHash}"
    '';

    # Optionally, if you want the git hash to be available inside your Rust code:
    meta = with lib; {
      description = "Crystal Forge";
      platforms = platforms.all;
    };
  };
in
  crystal-forge {inherit srcHash;}

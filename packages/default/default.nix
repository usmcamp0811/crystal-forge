{
  lib,
  pkgs,
  inputs,
  ...
}: let
  src = ./.;
  srcHash = builtins.hashString "sha256" (toString src);

  # Read and parse Cargo.toml to extract version
  cargoToml = builtins.fromTOML (builtins.readFile (src + "/Cargo.toml"));
  version = cargoToml.package.version;

  crystal-forge = pkgs.rustPlatform.buildRustPackage rec {
    inherit src version;
    pname = "crystal-forge";
    cargoLock = {
      lockFile = ./Cargo.lock;
    };

    # Ensure all dependencies are included
    nativeBuildInputs = with pkgs; [pkg-config];
    buildInputs = [
      pkgs.rustc
      pkgs.cargo
      pkgs.pkg-config
      pkgs.openssl
      pkgs.sqlx-cli
      pkgs.libressl # Ensure OpenSSL-related libraries are available
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
  crystal-forge // {inherit srcHash;}

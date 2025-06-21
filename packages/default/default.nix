{
  lib,
  pkgs,
  ...
}: let
  src = ./.;
  srcMigrations = src + /migrations;

  sqlx-db =
    pkgs.runCommand "sqlx-db-prepare" # 3
    
    {
      nativeBuildInputs = [pkgs.sqlx-cli];
    } ''
      mkdir $out
      export DATABASE_URL=sqlite:$out/db.sqlite3
      sqlx database create
      sqlx migrate --source ${srcMigrations} run
    '';

  crystal-forge = pkgs.naersk-lib.buildPackage {
    inherit src;
    pname = "crystal-forge";
    version = "0.1.0";

    doCheck = true;
    CARGO_BUILD_INCREMENTAL = "false";
    RUST_BACKTRACE = "full";
    copyLibs = false;

    overrideMain = old: {
      # 4
      linkDb = ''
        export DATABASE_URL=sqlite:${sqlx-db}/db.sqlite3            # 5
      '';

      preBuildPhases = ["linkDb"] ++ (old.preBuildPhases or []); # 6
    };
    nativeBuildInputs = with pkgs; [pkg-config];
  };
in
  crystal-forge

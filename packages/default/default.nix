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
  migrationsDir = ./migrations;
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
      pkgs.libressl
    ];

    # Runtime dependencies that need to be in PATH
    runtimeDeps = with pkgs; [
      util-linux # findmnt, blkid
      zfs # zfs command (optional)
      vulnix
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
  # data-only output with your .sql files staged in a stable path
  crystal-forge-migrations = pkgs.runCommand "crystal-forge-migrations" {} ''
    set -euo pipefail
    mkdir -p $out/share/crystal-forge/migrations
    cp -v ${migrationsDir}/*.sql $out/share/crystal-forge/migrations/
  '';

  # standalone CLI app to apply migrations (no inline in installPhase)

  migrate = pkgs.writeShellApplication {
    name = "crystal-forge-migrate";
    runtimeInputs = [pkgs.postgresql pkgs.coreutils pkgs.findutils pkgs.gawk];
    text = ''
      set -euo pipefail

      : "''${DATABASE_URL?Set DATABASE_URL, e.g. postgresql://postgres@127.0.0.1:5432/crystal_forge}"
      MIGDIR="''${MIGDIR:-${crystal-forge-migrations}/share/crystal-forge/migrations}"
      echo "Using migrations in: ''${MIGDIR}"

      # Build a NUL-safe, lexicographically sorted list without process substitution
      tmp_list="$(mktemp)"
      trap 'rm -f "''${tmp_list}"' EXIT

      # Find -> sort -z -> print lines (still NUL-safe via xargs -0)
      find "''${MIGDIR}" -maxdepth 1 -type f -name '*.sql' -print0 \
        | sort -z \
        | xargs -0 -I{} printf '%s\n' "{}" > "''${tmp_list}"

      if ! [ -s "''${tmp_list}" ]; then
        echo "No *.sql migrations found; nothing to do."
        exit 0
      fi

      while IFS= read -r f; do
        echo ">> applying $(basename "''${f}")"
        psql -v ON_ERROR_STOP=1 "''${DATABASE_URL}" -q -f "''${f}"
      done < "''${tmp_list}"

      echo "✅ migrations applied"
    '';
  };

  agent = pkgs.stdenv.mkDerivation {
    pname = "agent";
    version = pkgs.crystal-forge.default.version;
    src = pkgs.crystal-forge.default;
    installPhase = ''
      mkdir -p $out/bin
      cp ${crystal-forge}/bin/agent $out/bin/agent
      cp ${crystal-forge}/bin/cf-keygen $out/bin/cf-keygen
    '';
  };

  server = pkgs.stdenv.mkDerivation {
    pname = "server";
    version = pkgs.crystal-forge.default.version;
    src = pkgs.crystal-forge.default;
    installPhase = ''
      mkdir -p $out/bin
      cp ${pkgs.crystal-forge.default}/bin/server $out/bin/server
      cp ${pkgs.crystal-forge.default}/bin/cf-keygen $out/bin/cf-keygen
      cp ${pkgs.crystal-forge.default}/bin/builder $out/bin/builder
    '';
  };

  cf-keygen = pkgs.writeShellApplication {
    name = "cf-keygen";
    text = "${crystal-forge}/bin/cf-keygen \"$@\"";
  };
  test-agent = pkgs.writeShellApplication {
    name = "test-agent";
    text = "${crystal-forge}/bin/test-agent \"$@\"";
  };

  builder = pkgs.writeShellApplication {
    name = "builder";
    text = "${crystal-forge}/bin/builder \"$@\"";
  };
in
  crystal-forge // {inherit agent server builder cf-keygen test-agent srcHash migrate;}

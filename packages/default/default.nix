{ lib, pkgs, inputs, ... }:
let
  src = ./.;
  srcHash = builtins.hashString "sha256" (toString src);

  # Read and parse the server crate Cargo.toml to extract version
  # (root Cargo.toml is now a virtual workspace manifest with no [package])
  serverCargoToml = builtins.fromTOML (builtins.readFile (src + "/crates/cf-server/Cargo.toml"));
  version = serverCargoToml.package.version;
  migrationsDir = ./crates/cf-server/migrations;

  # ─────────────────────────────────────────────────────────────────────────
  # Source filtering
  #
  # Each component derivation filters its source to only the workspace metadata
  # and its transitive local-crate closure. This prevents server-only changes
  # from invalidating agent/builder derivations.
  # ─────────────────────────────────────────────────────────────────────────

  # All workspace member crate names (for workspace manifest inclusion)
  allCrateNames = [ "cf-protocol" "cf-config" "cf-agent" "cf-builder" "cf-keygen" "cf-server" ];

  # Helper: build a source filter that includes:
  # 1. Workspace root metadata (Cargo.toml, Cargo.lock)
  # 2. Cargo.toml of ALL workspace members (so Cargo can parse the workspace)
  # 3. Full source of the specified crate directories
  mkWorkspaceSrc = crates:
    lib.cleanSourceWith {
      src = src;
      filter = path: type:
        let
          relPath = lib.removePrefix (toString src + "/") (toString path);
          # Always include workspace root metadata
          isWorkspaceMeta =
            relPath == "Cargo.toml" ||
            relPath == "Cargo.lock";
          # Include the Cargo.toml manifest for every workspace member
          # (needed for Cargo to parse the workspace, even for packages not being built)
          isAnyMemberManifest = builtins.any
            (crate: relPath == "crates/${crate}/Cargo.toml" || relPath == "crates/${crate}")
            allCrateNames;
          # Include full source only for the crates in the component's closure
          isIncludedCrateSource = builtins.any
            (crate: lib.hasPrefix "crates/${crate}/" relPath)
            crates;
        in
        isWorkspaceMeta || isAnyMemberManifest || isIncludedCrateSource;
    };

  # Source closures for each component.
  # cf-protocol and cf-config are shared foundational crates needed by all.
  foundationalCrates = [ "cf-protocol" "cf-config" ];

  # Each component's filtered source: workspace metadata + transitive local deps source.
  agentSrc   = mkWorkspaceSrc (foundationalCrates ++ [ "cf-agent" ]);
  builderSrc = mkWorkspaceSrc (foundationalCrates ++ [ "cf-builder" ]);
  keygenSrc  = mkWorkspaceSrc [ "cf-keygen" ];
  serverSrc  = src; # server builds the full workspace

  # Common Rust build infrastructure
  commonBuildInputs = with pkgs; [
    pkg-config
    openssl
    libressl
  ];
  commonNativeBuildInputs = with pkgs; [ pkg-config ];

  # ─────────────────────────────────────────────────────────────────────────
  # Server derivation — builds cf-server with embedded-ui
  # ─────────────────────────────────────────────────────────────────────────
  cf-server-drv = pkgs.rustPlatform.buildRustPackage rec {
    src = serverSrc;
    inherit version;
    pname = "cf-server";
    cargoLock = { lockFile = ./Cargo.lock; };
    cargoBuildFlags = [ "--package" "cf-server" "--features" "cf-server/embedded-ui" ];
    CRYSTAL_FORGE_UI_DIST = "${pkgs.crystal-forge.web-ui}/public";

    nativeBuildInputs = commonNativeBuildInputs ++ (with pkgs; [ sqlx-cli ]);
    buildInputs = commonBuildInputs;

    # Runtime dependencies
    runtimeDeps = with pkgs; [
      util-linux # findmnt, blkid
      zfs        # optional
      vulnix
    ];

    preBuild = ''
      export SRC_HASH="${lib.strings.removeSuffix "\n" srcHash}"
    '';

    meta = with lib; {
      description = "Crystal Forge server";
      license = licenses.agpl3Only;
      platforms = platforms.all;
    };
  };

  # ─────────────────────────────────────────────────────────────────────────
  # Agent derivation — builds cf-agent only
  # Does NOT compile cf-server, cf-builder, or server-only dependencies.
  # ─────────────────────────────────────────────────────────────────────────
  cf-agent-drv = pkgs.rustPlatform.buildRustPackage rec {
    src = agentSrc;
    inherit version;
    pname = "cf-agent";
    cargoLock = { lockFile = ./Cargo.lock; };
    cargoBuildFlags = [ "--package" "cf-agent" ];

    nativeBuildInputs = commonNativeBuildInputs;
    buildInputs = commonBuildInputs;

    preBuild = ''
      export SRC_HASH="${lib.strings.removeSuffix "\n" srcHash}"
    '';

    meta = with lib; {
      description = "Crystal Forge deployment agent";
      license = licenses.agpl3Only;
      platforms = platforms.all;
    };
  };

  # ─────────────────────────────────────────────────────────────────────────
  # Builder derivation — builds cf-builder only
  # Does NOT compile cf-server or cf-agent packages.
  # ─────────────────────────────────────────────────────────────────────────
  cf-builder-drv = pkgs.rustPlatform.buildRustPackage rec {
    src = builderSrc;
    inherit version;
    pname = "cf-builder";
    cargoLock = { lockFile = ./Cargo.lock; };
    cargoBuildFlags = [ "--package" "cf-builder" ];

    nativeBuildInputs = commonNativeBuildInputs;
    buildInputs = commonBuildInputs;

    preBuild = ''
      export SRC_HASH="${lib.strings.removeSuffix "\n" srcHash}"
    '';

    meta = with lib; {
      description = "Crystal Forge remote build worker";
      license = licenses.agpl3Only;
      platforms = platforms.all;
    };
  };

  # ─────────────────────────────────────────────────────────────────────────
  # Key generation utility — builds cf-keygen only
  # ─────────────────────────────────────────────────────────────────────────
  cf-keygen-drv = pkgs.rustPlatform.buildRustPackage rec {
    src = keygenSrc;
    inherit version;
    pname = "cf-keygen";
    cargoLock = { lockFile = ./Cargo.lock; };
    cargoBuildFlags = [ "--package" "cf-keygen" ];

    nativeBuildInputs = commonNativeBuildInputs;
    buildInputs = commonBuildInputs;

    meta = with lib; {
      description = "Crystal Forge key generation utility";
      license = licenses.agpl3Only;
      platforms = platforms.all;
    };
  };

  # ─────────────────────────────────────────────────────────────────────────
  # Legacy monolithic build for backward compatibility.
  # Used by existing NixOS modules, test infrastructure, and CI that
  # reference pkgs.crystal-forge.default.
  # ─────────────────────────────────────────────────────────────────────────
  crystal-forge = pkgs.rustPlatform.buildRustPackage rec {
    inherit src version;
    pname = "crystal-forge";
    cargoLock = { lockFile = ./Cargo.lock; };
    cargoBuildFlags = [ "--features" "cf-server/embedded-ui" ];
    CRYSTAL_FORGE_UI_DIST = "${pkgs.crystal-forge.web-ui}/public";

    nativeBuildInputs = commonNativeBuildInputs ++ (with pkgs; [ sqlx-cli ]);
    buildInputs = commonBuildInputs;

    runtimeDeps = with pkgs; [
      util-linux
      zfs
      vulnix
    ];

    preBuild = ''
      export SRC_HASH="${lib.strings.removeSuffix "\n" srcHash}"
    '';

    meta = with lib; {
      description = "Crystal Forge";
      license = licenses.agpl3Only;
      platforms = platforms.all;
    };
  };

  # data-only output with migration SQL files
  crystal-forge-migrations = pkgs.runCommand "crystal-forge-migrations" { } ''
    set -euo pipefail
    mkdir -p $out/share/crystal-forge/migrations
    cp -v ${migrationsDir}/*.sql $out/share/crystal-forge/migrations/
  '';

  # standalone CLI app to apply migrations
  migrate = pkgs.writeShellApplication {
    name = "crystal-forge-migrate";
    runtimeInputs = [ pkgs.postgresql pkgs.coreutils pkgs.findutils pkgs.gawk ];
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

  # Component output derivations
  # These extract specific binaries from the dedicated component builds.

  agent = pkgs.stdenv.mkDerivation {
    pname = "agent";
    inherit version;
    src = cf-agent-drv;
    installPhase = ''
      mkdir -p $out/bin
      cp ${cf-agent-drv}/bin/agent $out/bin/agent
      cp ${cf-keygen-drv}/bin/cf-keygen $out/bin/cf-keygen
    '';
    meta = with lib; {
      description = "Crystal Forge deployment agent";
      license = licenses.agpl3Only;
      platforms = platforms.all;
    };
  };

  server = pkgs.stdenv.mkDerivation {
    pname = "server";
    inherit version;
    src = cf-server-drv;
    installPhase = ''
      mkdir -p $out/bin
      cp ${cf-server-drv}/bin/server $out/bin/server
      cp ${cf-server-drv}/bin/test-agent $out/bin/test-agent
      cp ${cf-keygen-drv}/bin/cf-keygen $out/bin/cf-keygen
      cp ${cf-builder-drv}/bin/builder $out/bin/builder
    '';
    meta = with lib; {
      description = "Crystal Forge server";
      license = licenses.agpl3Only;
      platforms = platforms.all;
    };
  };

  cf-keygen = pkgs.writeShellApplication {
    name = "cf-keygen";
    text = ''${cf-keygen-drv}/bin/cf-keygen "$@"'';
  };

  test-agent = pkgs.writeShellApplication {
    name = "test-agent";
    text = ''${cf-server-drv}/bin/test-agent "$@"'';
  };

  builder = pkgs.writeShellApplication {
    name = "builder";
    text = ''${cf-builder-drv}/bin/builder "$@"'';
  };

in crystal-forge // {
  inherit agent server builder cf-keygen test-agent srcHash migrate;
  inherit cf-server-drv cf-agent-drv cf-builder-drv cf-keygen-drv;
}

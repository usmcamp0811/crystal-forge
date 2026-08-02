{ lib, pkgs, inputs, ... }:
let
  src = ./.;

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

  # Helper: build a source filter that includes:
  # 1. Workspace root metadata (Cargo.toml, Cargo.lock)
  # 2. Full source of the specified crate directories
  mkWorkspaceSrc = crates:
    lib.cleanSourceWith {
      src = src;
      filter = path: _type:
        let
          relPath = lib.removePrefix (toString src + "/") (toString path);
          isWorkspaceMeta =
            relPath == "Cargo.toml" ||
            relPath == "Cargo.lock";
          # cleanSourceWith must retain ancestor directories before it can
          # descend into member manifests and selected crate sources.
          isWorkspaceDirectory = relPath == "crates";
          # Include full source only for the crates in the component's closure
          isIncludedCrateSource = builtins.any
            (crate:
              relPath == "crates/${crate}" ||
              lib.hasPrefix "crates/${crate}/" relPath)
            crates;
        in
        isWorkspaceMeta || isWorkspaceDirectory || isIncludedCrateSource;
    };

  # Cargo parses every member named by a workspace manifest, even when
  # --package selects a single component. Filtered builds therefore replace
  # the root manifest in the build tree with one containing only that
  # component's transitive local-crate closure. Excluded process manifests and
  # sources are neither parsed nor included in the derivation input.
  mkComponentWorkspaceManifest = name: crates:
    pkgs.writeText "${name}-workspace-Cargo.toml" ''
      [workspace]
      members = [
      ${lib.concatMapStringsSep "\n" (crate: "  \"crates/${crate}\",") crates}
      ]
      resolver = "2"
    '';

  # Source closures for each component.
  # cf-protocol and cf-config are shared foundational crates needed by all.
  foundationalCrates = [ "cf-protocol" "cf-config" ];

  agentSrc   = mkWorkspaceSrc (foundationalCrates ++ [ "cf-agent" ]);
  builderSrc = mkWorkspaceSrc (foundationalCrates ++ [ "cf-builder" ]);
  keygenSrc  = mkWorkspaceSrc [ "cf-keygen" ];
  serverSrc  = src; # server builds the full workspace

  agentWorkspaceManifest =
    mkComponentWorkspaceManifest "agent" (foundationalCrates ++ [ "cf-agent" ]);
  builderWorkspaceManifest =
    mkComponentWorkspaceManifest "builder" (foundationalCrates ++ [ "cf-builder" ]);
  keygenWorkspaceManifest =
    mkComponentWorkspaceManifest "keygen" [ "cf-keygen" ];

  # ─────────────────────────────────────────────────────────────────────────
  # Per-component SRC_HASH
  #
  # Each SRC_HASH is derived from that component's filtered source tree,
  # NOT from the full unfiltered backend. A server-only source change must
  # not change agentSrcHash or builderSrcHash.
  #
  # The builder and keygen do not use SRC_HASH at runtime, so they receive
  # no SRC_HASH preBuild export at all.
  # ─────────────────────────────────────────────────────────────────────────
  agentSrcHash   = builtins.hashString "sha256" (toString agentSrc);
  serverSrcHash  = builtins.hashString "sha256" (toString serverSrc);

  # Common Rust build infrastructure
  commonBuildInputs = with pkgs; [
    pkg-config
    openssl
    libressl
  ];
  commonNativeBuildInputs = with pkgs; [ pkg-config ];

  # ─────────────────────────────────────────────────────────────────────────
  # Server derivation — builds cf-server with embedded-ui
  # Only server and test-agent binaries; agent/builder/keygen are separate.
  # ─────────────────────────────────────────────────────────────────────────
  cf-server-drv = pkgs.rustPlatform.buildRustPackage rec {
    src = serverSrc;
    inherit version;
    pname = "cf-server";
    cargoLock = { lockFile = ./Cargo.lock; };
    cargoBuildFlags = [
      "--package" "cf-server"
      "--bin" "server"
      "--bin" "hardening-worker"
      "--bin" "test-agent"
      "--bin" "xccdf-export-fixture"
      "--features" "cf-server/embedded-ui"
    ];
    cargoCheckFlags = cargoBuildFlags;
    cargoTestFlags = [
      "--package" "cf-server"
      "--features" "cf-server/embedded-ui"
    ];
    CRYSTAL_FORGE_UI_DIST = "${pkgs.crystal-forge.web-ui}/public";

    nativeBuildInputs = commonNativeBuildInputs ++ (with pkgs; [ sqlx-cli ]);
    buildInputs = commonBuildInputs;

    runtimeDeps = with pkgs; [
      util-linux # findmnt, blkid
      zfs        # optional
      vulnix
    ];

    preBuild = ''
      export SRC_HASH="${lib.strings.removeSuffix "\n" serverSrcHash}"
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
  # SRC_HASH is derived from agentSrc only; server changes do not affect it.
  # ─────────────────────────────────────────────────────────────────────────
  cf-agent-drv = pkgs.rustPlatform.buildRustPackage rec {
    src = agentSrc;
    inherit version;
    pname = "cf-agent";
    cargoLock = { lockFile = ./Cargo.lock; };
    cargoBuildFlags = [ "--package" "cf-agent" ];
    cargoCheckFlags = cargoBuildFlags;
    cargoTestFlags = cargoBuildFlags;

    postPatch = ''
      cp ${agentWorkspaceManifest} Cargo.toml
    '';

    nativeBuildInputs = commonNativeBuildInputs;
    buildInputs = commonBuildInputs;

    # SRC_HASH is embedded in the agent binary via option_env!("SRC_HASH").
    # Use the agent-specific source hash so server-only changes do not
    # invalidate this derivation.
    preBuild = ''
      export SRC_HASH="${lib.strings.removeSuffix "\n" agentSrcHash}"
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
  # Does NOT set SRC_HASH; the builder binary does not embed a source hash.
  # ─────────────────────────────────────────────────────────────────────────
  cf-builder-drv = pkgs.rustPlatform.buildRustPackage rec {
    src = builderSrc;
    inherit version;
    pname = "cf-builder";
    cargoLock = { lockFile = ./Cargo.lock; };
    cargoBuildFlags = [ "--package" "cf-builder" ];
    cargoCheckFlags = cargoBuildFlags;
    cargoTestFlags = cargoBuildFlags;

    postPatch = ''
      cp ${builderWorkspaceManifest} Cargo.toml
    '';

    nativeBuildInputs = commonNativeBuildInputs;
    buildInputs = commonBuildInputs;

    # SRC_HASH intentionally not set: cf-builder does not use option_env!("SRC_HASH").

    meta = with lib; {
      description = "Crystal Forge remote build worker";
      license = licenses.agpl3Only;
      platforms = platforms.all;
    };
  };

  # ─────────────────────────────────────────────────────────────────────────
  # Key generation utility — builds cf-keygen only
  # Standalone; no SRC_HASH needed.
  # ─────────────────────────────────────────────────────────────────────────
  cf-keygen-drv = pkgs.rustPlatform.buildRustPackage rec {
    src = keygenSrc;
    inherit version;
    pname = "cf-keygen";
    cargoLock = { lockFile = ./Cargo.lock; };
    cargoBuildFlags = [ "--package" "cf-keygen" ];
    cargoCheckFlags = cargoBuildFlags;
    cargoTestFlags = cargoBuildFlags;

    postPatch = ''
      cp ${keygenWorkspaceManifest} Cargo.toml
    '';

    nativeBuildInputs = commonNativeBuildInputs;
    buildInputs = commonBuildInputs;

    meta = with lib; {
      description = "Crystal Forge key generation utility";
      license = licenses.agpl3Only;
      platforms = platforms.all;
    };
  };

  # ─────────────────────────────────────────────────────────────────────────
  # Legacy "crystal-forge" combined output for backward compatibility.
  #
  # Previously a single buildRustPackage; now a symlinkJoin of the four
  # dedicated component derivations. This preserves the existing flake
  # package name and binary layout that NixOS modules and test infra rely on,
  # while ensuring each binary is built from its authoritative crate.
  # ─────────────────────────────────────────────────────────────────────────
  crystal-forge = pkgs.symlinkJoin {
    name = "crystal-forge-${version}";
    inherit version;
    paths = [
      cf-server-drv
      cf-agent-drv
      cf-builder-drv
      cf-keygen-drv
    ];
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

  # Component output packages (expose dedicated builds at flake-level names)

  agent = pkgs.symlinkJoin {
    name = "crystal-forge-agent-${version}";
    inherit version;
    paths = [ cf-agent-drv cf-keygen-drv ];
  };

  server = pkgs.symlinkJoin {
    name = "crystal-forge-server-${version}";
    inherit version;
    paths = [ cf-server-drv cf-builder-drv cf-keygen-drv ];
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
  inherit agent server builder cf-keygen test-agent migrate;
  inherit cf-server-drv cf-agent-drv cf-builder-drv cf-keygen-drv;
  # Expose component-specific source hashes for verification
  agentSrcHash = agentSrcHash;
  serverSrcHash = serverSrcHash;
}

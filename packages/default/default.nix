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
  # Every component derivation filters its source to the workspace metadata
  # plus its transitive local-crate closure. A crate that no component depends
  # on is not part of that component's derivation input, so editing one
  # component does not invalidate the others.
  #
  # INVARIANT: the crate list passed to mkWorkspaceSrc must be the complete
  # transitive local-crate closure of the component. Cargo resolves path
  # dependencies from the build tree, so a missing crate is a build failure
  # rather than a silent fallback.
  # ─────────────────────────────────────────────────────────────────────────

  # Helper: build a source filter that includes:
  # 1. Workspace root metadata (Cargo.toml, Cargo.lock)
  # 2. Full source of the specified crate directories
  # 3. Any additional workspace-root paths the component needs at build time
  #
  # `rootPaths` names workspace-root entries that are real build inputs but are
  # not crate sources. Each entry is included together with everything below
  # it. Keep this list minimal: every entry is an invalidation edge, so a
  # component rebuilds whenever a listed path changes.
  mkWorkspaceSrc = { crates, rootPaths ? [ ] }:
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
          isIncludedRootPath = builtins.any
            (rootPath:
              relPath == rootPath ||
              lib.hasPrefix "${rootPath}/" relPath)
            rootPaths;
        in
        isWorkspaceMeta
        || isWorkspaceDirectory
        || isIncludedCrateSource
        || isIncludedRootPath;
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

  agentCrates   = foundationalCrates ++ [ "cf-agent" ];
  builderCrates = foundationalCrates ++ [ "cf-builder" ];
  keygenCrates  = [ "cf-keygen" ];
  # cf-server declares exactly two local path dependencies, cf-config and
  # cf-protocol. It does not depend on cf-agent, cf-builder, or cf-keygen, so
  # those crates are excluded from the server derivation input.
  serverCrates  = foundationalCrates ++ [ "cf-server" ];

  # The cf-server build expands sqlx query macros offline. sqlx resolves the
  # offline query cache from the workspace root, so `.sqlx` at the workspace
  # root is a required build input for the server and must survive filtering.
  #
  # COMPATIBILITY: `crates/cf-server/.sqlx` also exists but is a strict subset
  # of the workspace-root cache and is missing five queries. Excluding the root
  # cache fails the build with "set `DATABASE_URL` to use query macros online".
  # Do not remove this entry without first proving the crate-level cache is
  # complete. See TASK-451 for reconciling the two caches.
  #
  # The agent, builder, and keygen crates expand no query macros, so they do
  # not receive the query cache and are not invalidated when it changes.
  serverRootPaths = [ ".sqlx" ];

  agentSrc   = mkWorkspaceSrc { crates = agentCrates; };
  builderSrc = mkWorkspaceSrc { crates = builderCrates; };
  keygenSrc  = mkWorkspaceSrc { crates = keygenCrates; };
  serverSrc  = mkWorkspaceSrc {
    crates = serverCrates;
    rootPaths = serverRootPaths;
  };

  agentWorkspaceManifest =
    mkComponentWorkspaceManifest "agent" agentCrates;
  builderWorkspaceManifest =
    mkComponentWorkspaceManifest "builder" builderCrates;
  keygenWorkspaceManifest =
    mkComponentWorkspaceManifest "keygen" keygenCrates;
  serverWorkspaceManifest =
    mkComponentWorkspaceManifest "server" serverCrates;

  # ─────────────────────────────────────────────────────────────────────────
  # Per-component SRC_HASH
  #
  # Each SRC_HASH is derived from that component's filtered source tree, not
  # from the full backend tree. A server-only source change must not change
  # agentSrcHash, and an agent-only or builder-only change must not change
  # serverSrcHash.
  #
  # Each hash is taken over the filtered backend source store path. That path
  # encodes the content of the filtered tree, so the hash identifies the Rust
  # workspace sources available to that component build.
  #
  # SEMANTICS: SRC_HASH identifies the filtered backend workspace source of the
  # component that reports it. The server crate reads it in two places, and
  # both are source identity reporting rather than comparison against an
  # external value:
  #
  #   - handlers::api::admin reports it as the `commit` field of the server
  #     runtime info response.
  #   - models::system_states reports it as `agent_build_hash` for the
  #     test-agent binary, which the server crate also produces.
  #
  # Neither consumer compares SRC_HASH against a hash produced by another
  # component, so narrowing serverSrc does not break them. Both server variants
  # receive the same value. The hash does not identify Cargo feature selection
  # or embedded UI assets, which are outside serverSrc; a UI-only change can
  # therefore change the production server binary without changing SRC_HASH.
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

  craneLib = inputs.crane.mkLib pkgs;

  # ─────────────────────────────────────────────────────────────────────────
  # Server derivations
  #
  # The cf-server crate declares `embedded-ui` as an optional feature with no
  # default features, so the same crate source produces two builds:
  #
  #   cf-server-core-drv  no embedded UI, no dependency on the web-ui build
  #   cf-server-drv       embedded UI, depends on the web-ui build
  #
  # INVARIANT: the two variants differ only by the embedded-ui Cargo feature
  # and the CRYSTAL_FORGE_UI_DIST asset input. Every other build parameter is
  # shared, so a backend behavior difference between the variants is a defect.
  #
  # Why the split exists: CRYSTAL_FORGE_UI_DIST makes the web-ui derivation a
  # Nix input of the server. Without the split, editing a Dioxus source file
  # rebuilds the backend server and invalidates every check that boots a
  # server, including checks that never open a browser.
  #
  # Consumer rule:
  #
  #   - Use cf-server-core-drv for backend-only tests and checks. Those
  #     consumers must not gain a web-ui dependency.
  #   - Use cf-server-drv for the production package and for the authoritative
  #     browser check, which must prove the production binary serves the
  #     production WASM.
  #
  # Serving behavior without the feature: handlers::ui and the axum fallback
  # route at bin/server.rs are both gated on `embedded-ui`. A core build
  # therefore registers no UI fallback and answers a UI route with the axum
  # default 404 Not Found. API routes are unaffected. This is deliberate: a
  # core build must not appear to serve a UI.
  #
  # Only the server, hardening-worker, test-agent, and xccdf-export-fixture
  # binaries come from this crate. The agent, builder, and keygen binaries are
  # separate derivations.
  # ─────────────────────────────────────────────────────────────────────────
  serverCargoBuildExtraArgs = lib.concatStringsSep " " [
    "--bin server"
    "--bin hardening-worker"
    "--bin test-agent"
    "--bin xccdf-export-fixture"
  ];

  # Crane reads Cargo.toml files while constructing its dependency-only dummy
  # source, before any derivation hook can run. Replace the workspace manifest
  # in that dummy source explicitly, just as postPatch does in the real source.
  # This prevents excluded workspace members from becoming dependency inputs.
  serverDummySrc = craneLib.mkDummySrc {
    src = serverSrc;
    cargoLock = ./Cargo.lock;
    extraDummyScript = ''
      chmod +w $out/Cargo.toml
      cp ${serverWorkspaceManifest} $out/Cargo.toml
    '';
  };

  serverCommonArgs = {
    src = serverSrc;
    inherit version;
    strictDeps = true;

    postPatch = ''
      cp ${serverWorkspaceManifest} Cargo.toml
    '';

    nativeBuildInputs = commonNativeBuildInputs ++ (with pkgs; [ sqlx-cli ]);
    buildInputs = commonBuildInputs;

    runtimeDeps = with pkgs; [
      util-linux # findmnt, blkid
      zfs        # optional
      vulnix
    ];
  };

  # This derivation intentionally depends on Cargo.lock and the Cargo.toml
  # manifests only. craneLib.buildDepsOnly replaces Rust application source
  # with serverDummySrc, so editing a .rs file does not change this derivation.
  #
  # Build the superset of dependencies required by the embedded-UI variant.
  # The core variant can reuse that target tree without enabling the feature;
  # its closure still has no web-ui input because this feature selection adds
  # only the include_dir and mime_guess Rust crates, not UI assets.
  serverCargoArtifacts = craneLib.buildDepsOnly (
    builtins.removeAttrs serverCommonArgs [ "src" "postPatch" "runtimeDeps" ]
    // {
      pname = "cf-server";
      dummySrc = serverDummySrc;
      cargoExtraArgs = "--locked --package cf-server --features cf-server/embedded-ui";
      cargoBuildExtraArgs = serverCargoBuildExtraArgs;
      cargoTestExtraArgs = "--no-run --lib --bins";
    }
  );

  mkServerDrv = { pname, embedUi }:
    let
      cargoFeatureArg = lib.optionalString embedUi " --features cf-server/embedded-ui";
    in
    craneLib.buildPackage (serverCommonArgs // {
      inherit pname;
      cargoArtifacts = serverCargoArtifacts;
      cargoExtraArgs = "--locked --package cf-server${cargoFeatureArg}";
      cargoBuildExtraArgs = serverCargoBuildExtraArgs;
      cargoTestExtraArgs = "--lib --bins";
      preBuild = ''
        export SRC_HASH="${lib.strings.removeSuffix "\n" serverSrcHash}"
      '';

      meta = with lib; {
        description =
          if embedUi
          then "Crystal Forge server with embedded web UI"
          else "Crystal Forge server without embedded web UI";
        license = licenses.agpl3Only;
        platforms = platforms.all;
      };
    }
    # Setting CRYSTAL_FORGE_UI_DIST only for the embedded variant is what keeps
    # the web-ui derivation out of the core variant's input closure.
    # include_dir!("$CRYSTAL_FORGE_UI_DIST") is compiled only under the
    # embedded-ui feature, so a core build never reads this variable.
    // lib.optionalAttrs embedUi {
      CRYSTAL_FORGE_UI_DIST = "${pkgs.crystal-forge.web-ui}/public";
    });

  # Production server. Ships the embedded web UI.
  cf-server-drv = mkServerDrv {
    pname = "cf-server";
    embedUi = true;
  };

  # Backend-only server. Has no web-ui input, so Dioxus changes do not
  # invalidate it. Not a production artifact.
  cf-server-core-drv = mkServerDrv {
    pname = "cf-server-core";
    embedUi = false;
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

  # test-agent is a test harness binary and never serves the web UI, so it is
  # taken from the core server build. Sourcing it from the embedded build would
  # put the whole web-ui derivation in the closure of every test that runs a
  # fake agent.
  test-agent = pkgs.writeShellApplication {
    name = "test-agent";
    text = ''${cf-server-core-drv}/bin/test-agent "$@"'';
  };

  builder = pkgs.writeShellApplication {
    name = "builder";
    text = ''${cf-builder-drv}/bin/builder "$@"'';
  };

in crystal-forge // {
  inherit agent server builder cf-keygen test-agent migrate;
  # Component derivations. Internal consumers such as NixOS module services,
  # checks, and test helpers must reference these directly rather than the
  # `crystal-forge`, `server`, or `agent` aggregates above, so that a component
  # closure contains only what that consumer runs.
  inherit
    cf-server-drv
    cf-server-core-drv
    cf-agent-drv
    cf-builder-drv
    cf-keygen-drv
    serverCargoArtifacts
    ;
  # Expose component-specific source hashes for verification
  agentSrcHash = agentSrcHash;
  serverSrcHash = serverSrcHash;
}

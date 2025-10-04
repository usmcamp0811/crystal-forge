{
  lib,
  inputs,
  ...
}: {
  # adapted from https://github.com/charles-bord/nix-forest-slides/tree/master
  mkSlide = {
    pkgs,
    lib,
    stdenv,
    markdown,
    slides ? [],
    assets ? [],
    urlBase ? "/",
    extraNodePackages ? [],
    meta ? {},
  }: let
    cfg = {
      version = "0.50.0";
      rev = "v0.50.0";
      srcHash = "sha256-8LP7bAFWJAxd17u77aqX+j0mqTw59AODlrqot8np21g=";
      depsHash = "sha256-M9wqO+V5r2+PlxRMBe47fULTyaaeDWq45rR6XtKPsBw=";
      pnpm = pkgs.pnpm_9;
    };

    neversink-theme = lib.crystal-forge.buildPnpmTheme {
      inherit pkgs;
      pname = "slidev-theme-neversink";
      version = "0.3.6";
      src = pkgs.fetchFromGitHub {
        owner = "gureckis";
        repo = "slidev-theme-neversink";
        rev = "v0.3.6";
        hash = "sha256-JcdkZBcf059Pk5lqwGIlcTHmfIM54no98adeHe+TNBs=";
      };
      depsHash = "sha256-NKQ/MISoYnQFYMfcb8vOTE+YF1/AUHYRlGU4qNQalVY=";
      pnpm = pkgs.pnpm_9;
    };

    themes = [neversink-theme];

    slidev = pkgs.stdenv.mkDerivation {
      pname = "slidev";
      version = cfg.version;

      src = pkgs.fetchFromGitHub {
        owner = "slidevjs";
        repo = "slidev";
        rev = cfg.rev;
        hash = cfg.srcHash;
      };

      nativeBuildInputs = [pkgs.nodejs cfg.pnpm.configHook pkgs.makeWrapper];

      pnpmDeps = cfg.pnpm.fetchDeps {
        pname = "slidev";
        version = cfg.version;
        inherit (slidev) src;
        hash = cfg.depsHash;
        fetcherVersion = 1;
      };

      buildPhase = ''
        runHook preBuild
        pnpm build
        runHook postBuild
      '';

      installPhase = ''
        runHook preInstall
        mkdir -p $out
        cp -r packages $out/packages
        cp -r node_modules $out/node_modules
        makeWrapper ${pkgs.nodejs}/bin/node $out/bin/slidev \
          --set NODE_PATH "$out/node_modules" \
          --add-flags "$out/packages/slidev/bin/slidev.mjs"
        runHook postInstall
      '';

      postInstall = ''
        find $out -type l ! -exec test -e {} \; -print | xargs -r rm
      '';

      meta = {
        description = "Presentation Slides for Developers";
        homepage = "https://sli.dev/";
        changelog = "https://github.com/slidevjs/slidev/releases/tag/v${cfg.version}";
        mainProgram = "slidev";
      };
    };
  in
    stdenv.mkDerivation {
      pname = "slidev-presentation";
      version = "0.1.0";
      src = ./.;

      nativeBuildInputs = [slidev];

      buildInputs = extraNodePackages;

      buildPhase = let
        customThemeDirs = builtins.concatStringsSep "\n" (
          builtins.map
          (t: ''
            mkdir -p themes/${t.pname}
            cp -r ${t}/* themes/${t.pname}
          '')
          themes
        );
      in ''
        runHook preBuild

        mkdir themes

        ${customThemeDirs}

        chmod -R u+w themes/

        mkdir -p public/assets
        ${builtins.concatStringsSep "\n" (builtins.map (pkg: "cp -r ${pkg}/* public/assets/") assets)}

        mkdir -p slides
        ${builtins.concatStringsSep "\n" (builtins.map (pkg: "cp -r ${pkg}/* slides") slides)}

        mkdir -p node_modules

        # Copy all top-level packages from slidev
        cp -r ${slidev}/node_modules/* node_modules/

        # Inject extra packages (like sass-embedded)
        ${builtins.concatStringsSep "\n" (builtins.map (pkg: ''
            mkdir -p node_modules/${pkg.pname}
            cp -r ${pkg}/lib/node_modules/${pkg.pname}/* node_modules/${pkg.pname}/
          '')
          extraNodePackages)}

        cp ${markdown} ./slides.md
        slidev build --base "${urlBase}"

        runHook postBuild
      '';

      installPhase = ''
        runHook preInstall
        cp -r dist $out
        mkdir -p $out/themes
        cp -r themes $out/
        runHook postInstall
      '';

      meta =
        {
          description = "Slidev Presentation SPA";
          homepage = "https://sli.dev/";
          maintainers = with lib.maintainers; [];
        }
        // meta;
    };

  buildPnpmTheme = {
    pkgs,
    pname,
    version,
    src,
    depsHash,
    pnpm,
    meta ? {},
  }:
    pkgs.stdenv.mkDerivation {
      inherit pname version src;

      nativeBuildInputs = [pkgs.nodejs pnpm.configHook];

      pnpmDeps = pnpm.fetchDeps {
        inherit pname version src;
        hash = depsHash;
        fetcherVersion = 1;
      };

      installPhase = ''
        runHook preInstall
        cp -r . $out
        runHook postInstall
      '';

      meta =
        {
          description = "Built theme ${pname}";
        }
        // meta;
    };

  buildNpmTheme = {
    pkgs,
    pname,
    version,
    src,
    depsHash ? null,
    peerDeps ? {},
    meta ? {},
  }:
    pkgs.buildNpmPackage {
      inherit pname version src;
      npmDepsHash = depsHash;

      env = {
        PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD = "1";
      };

      preBuild = ''
        echo "Injecting peerDependencies..."
        tmpfile=$(mktemp)
        ${pkgs.jq}/bin/jq --argjson peerDeps '${builtins.toJSON peerDeps}' '
          .dependencies += $peerDeps
        ' package.json > $tmpfile
        mv $tmpfile package.json
      '';

      installPhase = ''
        runHook preInstall
        cp -r . $out
        runHook postInstall
      '';

      meta =
        {
          description = "Built theme ${pname}";
        }
        // meta;
    };

  buildYarnTheme = {
    pkgs,
    pname,
    version,
    src,
    yarnNix,
    meta ? {},
  }: let
    themePkg = pkgs.stdenv.mkDerivation rec {
      inherit pname version;

      buildInputs = [
        (pkgs.yarn2nix-moretea.mkYarnPackage {
          inherit pname version src yarnNix;
          packageJSON = "${src}/package.json";
          yarnLock = "${src}/yarn.lock";
        })
      ];

      phases = ["installPhase"];

      installPhase = ''
        runHook preInstall
        mkdir -p $out/deps/${pname}
        cp -r ${builtins.head buildInputs}/libexec/${pname}/* $out
        runHook postInstall
      '';

      meta = {
        description = "Raw theme build for ${pname}";
      };
    };
  in
    pkgs.stdenv.mkDerivation {
      inherit pname version;
      src = themePkg;

      phases = ["installPhase"];

      installPhase = ''
        mkdir -p $out
        ln -s ${themePkg}/deps/${pname}/* $out/
      '';

      meta =
        {
          description = "Slidev theme ${pname}";
        }
        // meta;
    };
}

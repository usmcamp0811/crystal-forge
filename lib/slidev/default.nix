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
    customCss ? null,
    meta ? {},
  }: let
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
      depsHash = "sha256-n1VIwehFBeIAceCsn15wJSi3AX2IHw5GMP9RsW8AhWc=";
      pnpm = pkgs.pnpm_10;
    };

    themes = [neversink-theme];

    # Use nixpkgs slidev-cli (now available in 26.05), wrapped for compatibility
    slidev = pkgs.runCommand "slidev-compat" {} ''
      mkdir -p $out/bin
      mkdir -p $out/node_modules
      ln -s ${pkgs.slidev-cli}/bin/slidev $out/bin/slidev
      ln -s ${pkgs.slidev-cli}/lib/node_modules/slidev-cli/node_modules/* $out/node_modules/
    '';
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
        # Handle both the compat wrapper and direct slidev-cli
        if [ -L "${slidev}/node_modules" ]; then
          # Compat wrapper with symlinks
          cp -rL ${slidev}/node_modules/* node_modules/
        elif [ -d "${slidev}/node_modules" ]; then
          # Direct node_modules directory
          cp -r ${slidev}/node_modules/* node_modules/
        elif [ -d "${slidev}/lib/node_modules/slidev-cli/node_modules" ]; then
          # nixpkgs slidev-cli structure
          cp -r ${slidev}/lib/node_modules/slidev-cli/node_modules/* node_modules/
        else
          echo "Error: Could not find node_modules in slidev package"
          exit 1
        fi

        # Inject extra packages (like sass-embedded)
        ${builtins.concatStringsSep "\n" (builtins.map (pkg: ''
            mkdir -p node_modules/${pkg.pname}
            cp -r ${pkg}/lib/node_modules/${pkg.pname}/* node_modules/${pkg.pname}/
          '')
          extraNodePackages)}

        cp ${markdown} ./slides.md
        mkdir -p ./styles
        ${
          if customCss != null
          then "cp ${customCss} ./styles/index.css"
          else ""
        }
        
        # Create vite.config.ts to allow assets from public directory
        cat > vite.config.ts <<EOF
        import { defineConfig } from 'vite'
        
        export default defineConfig({
          server: {
            fs: {
              strict: false,
              allow: ['.', '/build']
            }
          }
        })
        EOF
        
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
    pnpm ? pkgs.pnpm_10,
    meta ? {},
  }:
    pkgs.stdenv.mkDerivation {
      inherit pname version src;

      nativeBuildInputs = [pkgs.nodejs pkgs.pnpmConfigHook pnpm];

      pnpmDeps = pkgs.fetchPnpmDeps {
        inherit pname version src pnpm;
        hash = depsHash;
        fetcherVersion = 3;
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

{ lib, pkgs, inputs, ... }:
let
  root = ../..;
  repositoryUrl = "https://gitlab.com/crystal-forge/crystal-forge";
  python = pkgs.python3.withPackages (p: [
    p.pyyaml p.markdown-it-py p.mdit-py-plugins p.pygments
  ]);
  options = import ./options.nix {
    inherit pkgs inputs;
    repositoryUrl = "${repositoryUrl}/-/blob/dev";
  };
  mermaid = pkgs.fetchurl {
    # Same pinned browser bundle as the dotfiles OKF publisher.
    url = "https://unpkg.com/mermaid@11.12.0/dist/mermaid.min.js";
    hash = "sha256-B+N9+pezN8zIU2XVft35m5cG8J2ztZsmDQMzsjs0PEs=";
  };
  docs = lib.fileset.toSource {
    inherit root;
    # Preserve repository-relative image paths. The renderer publishes only
    # images referenced by the explicit knowledge/options bundles.
    fileset = lib.fileset.unions [
      (lib.fileset.fileFilter (file: file.hasExt "md") (root + "/docs/knowledge"))
      (lib.fileset.fileFilter
        (file: lib.any file.hasExt [ "png" "jpg" "jpeg" "gif" "svg" "webp" "avif" "ico" ])
        (root + "/docs"))
    ];
  };
  scripts = lib.fileset.toSource {
    root = ./.;
    fileset = lib.fileset.unions [
      (lib.fileset.fileFilter (file: file.hasExt "py") ./.)
      ./assets
      ./README.md
    ];
  };
  tests = pkgs.runCommand "docs-site-tests" {
    passthru.browser = import ./browser.nix { inherit pkgs site; };
    nativeBuildInputs = [ python ];
    DOCS_OPTIONS_JSON = "${options.optionsJSON}/share/doc/nixos/options.json";
    DOCS_OPTIONS_BUNDLE = options;
  } ''
    export PYTHONDONTWRITEBYTECODE=1
    python -m unittest discover -s ${scripts} -p 'test_*.py' -v
    touch "$out"
  '';
  site = pkgs.stdenvNoCC.mkDerivation {
  pname = "docs-site";
  version = "1";
  dontUnpack = true;
  nativeBuildInputs = [ python ];
  installPhase = ''
    runHook preInstall
    # Tests are a build dependency, including real generated-option coverage.
    test -e ${tests}
    python ${scripts}/build.py --out "$out/share/docs-site" \
      --mermaid ${mermaid} --repository-url ${lib.escapeShellArg repositoryUrl} \
      --repository-root ${docs} \
      ${docs}/docs/knowledge ${options}/options
    mkdir -p "$out/bin"
    cat > "$out/bin/docs-site" <<EOF
    #!${pkgs.runtimeShell}
    exec ${python}/bin/python -m http.server "\''${1:-8754}" --bind 127.0.0.1 --directory "$out/share/docs-site"
    EOF
    chmod +x "$out/bin/docs-site"
    runHook postInstall
  '';
  passthru = { inherit options tests python mermaid; };
  meta = {
    description = "Static Crystal Forge knowledge and generated NixOS module reference";
    mainProgram = "docs-site";
    platforms = lib.platforms.all;
  };
  };
in
site

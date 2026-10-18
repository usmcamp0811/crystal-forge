{pkgs, lib, ...}: let
  # Public dotfiles nixos/flake.lock, fetched 2026-10-10. The onboarding
  # expression receives caller pkgs; test both root locked package sets.
  lockedPackages = rev: hash:
    import (builtins.fetchTarball {
      url = "https://github.com/NixOS/nixpkgs/archive/${rev}.tar.gz";
      sha256 = hash;
    }) {system = pkgs.stdenv.hostPlatform.system;};
  onboard = lockedPackages "36d0315f01000f976b9c37baf72910b1389983e4"
    "sha256-BjkaWLl/urJkcw8GF7LWnYxvEPRFPZfz2AaeGsXVSUo=";
  unstable = lockedPackages "e7439b6b14ad3cc35d05608ebca9bce01a25f5f8"
    "sha256-H83yz/do+hjOO0uujHyH8RgtIQP2v7Mw+cLwcZK/xKc=";
  runner = pkgs.rustPlatform.buildRustPackage {
    pname = "cf-ca-bundle-test-runner";
    version = "0.1.0";
    src = lib.cleanSourceWith {
      src = ../../packages/default;
      filter = path: _: let
        rel = lib.removePrefix (toString ../../packages/default + "/") (toString path);
      in rel == "Cargo.lock" || rel == "Cargo.toml" || rel == "crates"
        || lib.hasPrefix "crates/cf-config" rel || lib.hasPrefix "crates/cf-protocol" rel
        # cf-config's existing cfg(test) compatibility module imports this file.
        || lib.elem rel ["crates/cf-server" "crates/cf-server/src" "crates/cf-server/src/config" "crates/cf-server/src/config/server.rs"];
    };
    cargoLock.lockFile = ../../packages/default/Cargo.lock;
    postPatch = ''
      cat > Cargo.toml <<'EOF'
      [workspace]
      members = ["crates/cf-config", "crates/cf-protocol"]
      resolver = "2"
      EOF
    '';
    nativeBuildInputs = [pkgs.pkg-config];
    buildInputs = [pkgs.openssl];
    cargoBuildFlags = ["--package" "cf-config" "--example" "normalize_ca_bundle"];
    # Existing synthetic-key unit tests execute only inside this sandbox.
    cargoTestFlags = ["--package" "cf-config" "--lib" "ca_bundle::tests"];
    installPhase = ''
      mkdir -p "$out/bin"
      cp target/${pkgs.stdenv.hostPlatform.rust.rustcTarget}/release/examples/normalize_ca_bundle "$out/bin/"
    '';
  };
  datasets = {cf = pkgs.cacert; dotfiles = onboard.cacert; dotfiles-unstable = unstable.cacert;};
  provenance = lib.mapAttrs (_: ca: {
    inherit (ca) version;
    package = toString ca;
    source = toString ca.src;
    sourceHash = ca.src.outputHash;
  }) datasets;
  fixture = pkgs.runCommand "crystal-forge-onboarding-public-ca-fixture" {
    nativeBuildInputs = [pkgs.go pkgs.openssl pkgs.coreutils];
  } ''
    # All key generation takes place in the Nix sandbox's verified TMPDIR.
    test -d "$TMPDIR"
    umask 077
    mkdir -p "$out" "$TMPDIR/go-cache"
    trap 'rm -f "$TMPDIR/root.key"' EXIT
    openssl req -x509 -newkey ec -pkeyopt ec_paramgen_curve:prime256v1 -nodes -days 1 \
      -subj '/CN=TASK-470 synthetic onboarding root' \
      -addext 'basicConstraints=critical,CA:TRUE' \
      -keyout "$TMPDIR/root.key" -out "$TMPDIR/root.pem" >/dev/null 2>&1
    rm -f "$TMPDIR/root.key"
    export GOCACHE="$TMPDIR/go-cache" HOME="$TMPDIR" CGO_ENABLED=0
    go build -o "$TMPDIR/compare" ${./compare.go}
    cp "$TMPDIR/compare" "$out/compare"
    cp "$TMPDIR/root.pem" "$out/root.pem"
    cp ${pkgs.writeText "ca-provenance.json" (builtins.toJSON provenance)} "$out/provenance.json"
    ${lib.concatStringsSep "\n" (lib.mapAttrsToList (name: ca: ''
      public=${ca}/etc/ssl/certs/ca-bundle.crt
      sha256sum "$public" > "$out/${name}-bundle.sha256"
      "$TMPDIR/compare" "$public" > "$out/${name}-public.json"
      cat "$out/${name}-public.json"
      cat "$TMPDIR/root.pem" "$public" > "$out/${name}.pem"
      "$TMPDIR/compare" "$public" "$out/${name}.pem" "$TMPDIR/root.pem" > "$out/${name}-report.json"
      cat "$out/${name}-report.json"
    '') datasets)}
    if cmp -s ${pkgs.cacert}/etc/ssl/certs/ca-bundle.crt ${onboard.cacert}/etc/ssl/certs/ca-bundle.crt; then
      printf 'CF and dotfiles root nixpkgs bundles: byte-identical\n'
    else
      printf 'CF and dotfiles root nixpkgs bundles: different\n'
    fi
    cmp ${pkgs.cacert}/etc/ssl/certs/ca-bundle.crt ${unstable.cacert}/etc/ssl/certs/ca-bundle.crt
  '';
in pkgs.runCommand "crystal-forge-niks3-ca-bundle" {
  nativeBuildInputs = [pkgs.openssl];
  passthru = {inherit runner fixture provenance;};
} ''
  mkdir -p "$out"
  ${lib.concatStringsSep "\n" (lib.mapAttrsToList (name: ca: ''
    ${runner}/bin/normalize_ca_bundle ${fixture}/${name}.pem "$out/${name}-normalized.pem"
    ${fixture}/compare ${ca}/etc/ssl/certs/ca-bundle.crt ${fixture}/${name}.pem \
      "$out/${name}-normalized.pem" ${fixture}/root.pem > "$out/${name}-report.json"
    cat "$out/${name}-report.json"
  '') datasets)}
''

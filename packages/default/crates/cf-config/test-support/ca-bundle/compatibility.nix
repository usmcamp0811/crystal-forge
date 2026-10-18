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
  datasets = {cf = pkgs.cacert; dotfiles = onboard.cacert; dotfiles-unstable = unstable.cacert;};
  provenance = lib.mapAttrs (_: ca: {
    inherit (ca) version;
    package = toString ca;
    source = toString ca.src;
    sourceHash = ca.src.outputHash;
  }) datasets;
in ''
    # Reuse this component's Cargo target tree. The normal core library run owns
    # ca_bundle::tests; this command only builds the dataset normalization CLI.
    cargo build --release --locked --offline --package cf-config \
      --example normalize_ca_bundle --message-format=json > "$TMPDIR/ca-normalizer-build.jsonl"
    runner=$(jq -er 'select(.reason == "compiler-artifact" and .target.name == "normalize_ca_bundle" and .executable != null) | .executable' \
      "$TMPDIR/ca-normalizer-build.jsonl")
    fixture="$TMPDIR/ca-bundle-compatibility"
    # All key generation takes place in the Nix sandbox's verified TMPDIR.
    test -d "$TMPDIR"
    umask 077
    mkdir -p "$fixture" "$TMPDIR/go-cache"
    trap 'rm -f "$TMPDIR/root.key"' EXIT
    openssl req -x509 -newkey ec -pkeyopt ec_paramgen_curve:prime256v1 -nodes -days 1 \
      -subj '/CN=TASK-470 synthetic onboarding root' \
      -addext 'basicConstraints=critical,CA:TRUE' \
      -keyout "$TMPDIR/root.key" -out "$TMPDIR/root.pem" >/dev/null 2>&1
    rm -f "$TMPDIR/root.key"
    export GOCACHE="$TMPDIR/go-cache" HOME="$TMPDIR" CGO_ENABLED=0
    go build -o "$TMPDIR/compare" ${./compare.go}
    cp ${pkgs.writeText "ca-provenance.json" (builtins.toJSON provenance)} "$fixture/provenance.json"
    ${lib.concatStringsSep "\n" (lib.mapAttrsToList (name: ca: ''
      public=${ca}/etc/ssl/certs/ca-bundle.crt
      sha256sum "$public" > "$fixture/${name}-bundle.sha256"
      cat "$TMPDIR/root.pem" "$public" > "$fixture/${name}.pem"
      "$runner" "$fixture/${name}.pem" "$fixture/${name}-normalized.pem"
      "$TMPDIR/compare" "$public" "$fixture/${name}.pem" \
        "$fixture/${name}-normalized.pem" "$TMPDIR/root.pem" > "$fixture/${name}-report.json"
      cat "$fixture/${name}-report.json"
    '') datasets)}
    if cmp -s ${pkgs.cacert}/etc/ssl/certs/ca-bundle.crt ${onboard.cacert}/etc/ssl/certs/ca-bundle.crt; then
      printf 'CF and dotfiles root nixpkgs bundles: byte-identical\n'
    else
      printf 'CF and dotfiles root nixpkgs bundles: different\n'
    fi
    cmp ${pkgs.cacert}/etc/ssl/certs/ca-bundle.crt ${unstable.cacert}/etc/ssl/certs/ca-bundle.crt
''

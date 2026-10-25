{ lib, pkgs, ... }:
pkgs.rustPlatform.buildRustPackage {
  pname = "crystal-forge-oscal-fixture";
  version = "0.3.0";

  src = ./.;

  cargoLock = {
    lockFile = ./Cargo.lock;
  };

  nativeCheckInputs = [
    (pkgs.python3.withPackages (p: [ p.jsonschema p.regex ]))
  ];
  # Validate the actual fixture binary and its AR -> AP -> SSP document chain.
  # Rust unit tests do not validate the external NIST schema contract.
  postCheck = ''
    target/${pkgs.stdenv.hostPlatform.rust.rustcTarget}/release/crystal-forge-oscal-fixture \
      > "$TMPDIR/assessment-results.json"
    python3 ${./validate.py} \
      --assessment-results "$TMPDIR/assessment-results.json" \
      --schema-dir ${pkgs.crystal-forge.oscal-1-1-2-schemas}
  '';

  meta = {
    description = "Deterministic OSCAL 1.1.2 Assessment Results fixture generator";
    mainProgram = "crystal-forge-oscal-fixture";
  };
}

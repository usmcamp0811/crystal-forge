{ lib, pkgs, ... }:
let
  inherit (pkgs) python3;
  oscalFixture = pkgs.crystal-forge.oscal-fixture;
  oscalSchemas = pkgs.crystal-forge.oscal-1-1-2-schemas;
in
pkgs.runCommand "oscal-export-validation" {
  nativeBuildInputs = [
    oscalFixture
    (python3.withPackages (p: [ p.jsonschema ]))
  ];
  meta = {
    description = "Validate Crystal Forge OSCAL 1.1.2 export against NIST schemas";
  };
} ''
  # Generate deterministic OSCAL Assessment Results
  crystal-forge-oscal-fixture > "$TMPDIR/assessment-results.json"

  # Validate the document chain (AR -> AP -> SSP) against NIST 1.1.2 schemas
  python3 ${./validate.py} \
    --assessment-results "$TMPDIR/assessment-results.json" \
    --schema-dir ${oscalSchemas}

  touch "$out"
''

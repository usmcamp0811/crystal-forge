# OSCAL Export Check

This check validates a deterministic, fixture-generated OSCAL Assessment
Results document against the vendored NIST OSCAL 1.1.2 JSON schemas, and
against the AP and SSP documents it references.

## What it verifies

- `crystal-forge-oscal-fixture` (the same fixture generator used elsewhere
  for deterministic OSCAL content) produces an Assessment Results document.
- `validate.py` validates that document, and the Assessment Plan and System
  Security Plan documents it chains to (AR → AP → SSP), against the vendored
  NIST 1.1.2 JSON Schemas using `jsonschema` and `regex`.

## Why it is a separate check

OSCAL schema validation is a data-shape contract independent of the browser.
Validating it directly against the fixture generator, rather than only
through the `web-ui` check's browser-triggered download, keeps this
regression fast, network-free, and decoupled from the web UI build.
End-to-end coverage of the actual `build_oscal()` WASM code path used by a
real download still exists separately in the `web-ui` check's Phase 5.

## Run it

```sh
nix build .#checks.x86_64-linux.oscal-export --print-build-logs
```

No VM is involved. `python3` with `jsonschema` and `regex` runs inside the
build sandbox against the vendored schema derivation
(`pkgs.crystal-forge.oscal-1-1-2-schemas`); no network access is needed at
build time.

## Out of scope

- This does not exercise the web UI's export modal, browser-triggered
  download, or the production WASM `build_oscal()` code path. See the
  `web-ui` check's Phase 5 (OSCAL Export Validation) for that end-to-end
  coverage.

## CI

Not present in the `.gitlab-ci.yml` `flake-check` matrix by name; it runs as
part of a broader `nix flake check` where applicable.

## Related files

- `validate.py` — the schema-validation script invoked by `default.nix`.

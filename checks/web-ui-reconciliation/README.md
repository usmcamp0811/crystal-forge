# Web UI Reconciliation Check

This check runs one focused Playwright workflow
(`20ac-stig-import-reconciliation-fixture`) from the `web-ui` check's own
`tests/integration-test.js` harness, but against nginx serving only the
static production web-ui build — there is no Crystal Forge server, no
database, and no backend at all. Every API call the workflow needs is
route-mocked by Playwright.

## What it verifies

- The production web-ui build (`pkgs.crystal-forge.web-ui`) serves a valid
  `index.html` that references a JS loader, which is itself served, and the
  referenced `.wasm` output has a valid WebAssembly magic header (the same
  three checks `web-ui`'s `verifyWebUiAssets` performs, done here directly
  against static files behind nginx).
- The single `20ac-stig-import-reconciliation-fixture` workflow passes with
  both its light and dark screenshots captured, using `CF_UI_TEST_STANDALONE=1`
  so the harness relies entirely on route mocks rather than a real backend.

## Why it is a separate check

This isolates one specific STIG-import reconciliation UI workflow from the
cost of the full `web-ui` mega VM (real server, database, builder, gitserver,
and every other workflow in the manifest). It reuses the exact same test
file and manifest as `web-ui` — it copies
`checks/web-ui/tests/integration-test.js` and
`checks/web-ui/coverage-manifest.json` into the VM — so a workflow written
once in `web-ui`'s harness can also run here in standalone mode without
duplicating test logic.

## Run it

```sh
nix build .#checks.x86_64-linux.web-ui-reconciliation --print-build-logs
```

Boots one lightweight NixOS VM running nginx and the static web-ui build;
no PostgreSQL, no Crystal Forge server or agent, no gitserver.

## Out of scope

- Only the one named workflow runs here; this is not a substitute for the
  `web-ui` check's full manifest coverage.
- Any workflow that needs a real backend response (rather than a Playwright
  route mock) cannot run through this check.

## CI

Not present in the `.gitlab-ci.yml` `flake-check` matrix by name; it runs as
part of a broader `nix flake check` where applicable.

## Related files

- `../web-ui/tests/integration-test.js` — the shared Playwright harness this
  check copies into the VM and runs in standalone mode.
- `../web-ui/coverage-manifest.json` — the shared workflow manifest this
  check copies into the VM.

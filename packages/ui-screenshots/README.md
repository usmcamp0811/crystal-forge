# UI Screenshots Package

This is a lightweight, backend-free visual check for the Dioxus web UI. It
serves the pre-built production WASM bundle, intercepts every `/api/v1/`
call with fixture JSON, and screenshots each configured view in both light
and dark themes. No Crystal Forge server, database, or network is involved.

It is exposed as `packages.x86_64-linux.ui-screenshots` so its output PNGs
can be built and inspected directly.

## What it verifies

- `capture.js` drives Playwright against the served production web-ui
  build, using fixture data from
  `docs/design/CrystalForge/fixtures/crystal-forge.fixtures.json`, and
  writes one PNG per view/theme combination into the derivation output.

## Why it is a separate package

This gives a fast, deterministic way to produce or inspect current
screenshots of every view without paying for a real backend, database, or
the `web-ui` check's full manifest of semantic assertions. It is the
cheapest way to visually eyeball a Dioxus rendering change.

## Run it

```sh
nix build .#ui-screenshots --print-build-logs
ls result/
```

Runs entirely inside the Nix build sandbox with `__noChroot = true` (needed
for the bundled Chromium); no separate VM is booted.

## Out of scope

- No semantic assertions, no interactive workflows (clicking, form
  submission, navigation), and no real API responses — this only proves
  that each view renders and can be captured against static fixture data.
  See the `web-ui` check for behavioral coverage.

## CI

Build this package explicitly to generate screenshots. `nix flake check`
does not build it.

## Related files

- `capture.js` — the Playwright screenshot driver.
- `routes.js`, `generate-fixture-routes.js` — route/fixture wiring consumed
  by the capture script.
- `seed-db.spec.ts`, `playwright.config.ts`, `tsconfig.json` — supporting
  Playwright/TypeScript configuration for this check's tooling.

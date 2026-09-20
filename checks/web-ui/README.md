# Web UI Check

This is the authoritative pre-merge gate for the web UI. It boots a NixOS
VM running the **production embedded-UI server build**
(`cf-server-drv`, not the core build used by `integration` and `oidc-auth`),
the agent, the builder, a Git server, and — only when explicitly
enabled — Attic and S3-compatible cache VMs. It drives the real web UI
through Playwright, against the manifest of workflows declared in
`coverage-manifest.json`.

Do not change the production server package binding
(`cfServer = pkgs.crystal-forge.default.cf-server-drv;`) to the core build.
This is the one check that proves the shipped server binary serves the
shipped production WASM bundle through a real browser; using the core build
here would silently remove that guarantee.

## What it verifies

- **Build verification** — served `index.html` references a JS loader, the
  loader is served, and the referenced packaged `.wasm` has a valid
  WebAssembly magic header, checked directly against the production build's
  output (`verifyWebUiAssets`).
- **Semantic assertions and screenshots per manifest step** — every step
  named in `tests/integration-test.js` must exist in
  `coverage-manifest.json`, and vice versa; the check fails outright
  (`fatal.json`) on manifest/test drift before any workflow runs.
- **A fixed list of critical workflows** (see `critical_tests` in
  `default.nix`) must be present in the results and must pass. This
  includes, among many others, login/registration, system management and
  TASK-435 agent key-rotation authorization/persistence workflows, flakes,
  builds, CVE triage, evaluations, POA&M lifecycle and bulk workflows, admin
  automatic-retry settings, evidence lifecycle, and the full policy/STIG
  authoring and mapping-round-trip family. Non-critical workflows in the
  manifest may fail without failing the check, but critical ones cannot.
- **Strict visual baselines** — manifest steps marked `strict` must match
  their approved baseline in `baselines/` within threshold, or the check
  fails; `advisory` steps are reported with diff images but never block.
- **OSCAL export validation (Phase 5)** — routes real compliance API data,
  opens the export modal in the real web UI, captures the browser-triggered
  download, and validates it against the vendored NIST OSCAL 1.1.2 AR/AP/SSP
  schemas. This exercises the actual production `build_oscal()` WASM code
  path, the file a user would actually download — a stronger guarantee than
  the fixture-only `oscal-export` check provides.
- **SARIF export validation (Phase 6)** — the same end-to-end pattern for
  SARIF 2.1.0, validated against the vendored OASIS Errata 01 schema with
  format checking and semantic checks (rule-ID resolution, host locations,
  waiver suppressions).
- **Design-parity visual comparison** — renders the tracked design example
  (`docs/design/CrystalForge`, vendored offline) and compares it against the
  real Dioxus captures. This is reported as a drift gauge and summary matrix
  and is explicitly non-blocking; it never fails the check on its own.

## Run it

```sh
nix build .#checks.x86_64-linux.web-ui --print-build-logs
```

To run a subset of workflows (much faster iteration):

```sh
CF_UI_TEST_STEPS="16-cves,16b-cves-severity-filter" \
  nix build --impure .#checks.x86_64-linux.web-ui --no-link -L
```

`--impure` is required whenever `CF_UI_TEST_STEPS` (or other environment
overrides consumed via `builtins.getEnv`) should actually take effect,
because `testSteps` defaults to reading `CF_UI_TEST_STEPS` from the
environment. Global timeout is 2400 seconds (40 minutes) for the full
manifest; `playwrightResultTimeout` (default 1800s) additionally bounds how
long the check waits for the Playwright process's own exit marker.

Useful environment variables (all require `--impure` to take effect):

- `CF_UI_TEST_STEPS` — comma-separated workflow names to run instead of the
  full manifest.
- `CF_UI_TEST_PROFILE` — defaults to `ci_fast`.
- `CF_UI_UPDATE_BASELINES=1` — export strict-baseline candidates instead of
  failing on visual mismatch (used by the manual
  `web-ui-baseline-candidates` CI job; review and approve candidates with
  `approve-baselines.sh` before committing them).
- `CF_WEB_UI_RUN_MEGA_PHASES=1` — also boots the Attic and S3 cache VMs and
  runs the legacy cache/builder pytest phases. Interactive/manual use only;
  this variable cannot cross the Nix build sandbox in a normal CI run.

## Out of scope

- OIDC authentication (`oidc-auth` check).
- Rust-only PostgreSQL regressions not reachable through the browser
  (`server-regressions` check).
- The Attic/S3/builder pytest phases are opt-in and skipped by default; they
  are legacy coverage retained for interactive use, not part of the default
  gate.
- Design-parity comparison is diagnostic only; it does not enforce visual
  match against the design example the way strict baselines do.

## CI

Part of the `.gitlab-ci.yml` `flake-check` matrix (`CHECK_NAME: web-ui`), so
it runs on every merge request and on `main`. Its screenshots are copied
into CI artifacts and posted as an MR comment by the separate
`web-ui-screenshots-mr-comment` job. `web-ui-baseline-candidates` is a
manual, allow-failure job that runs this same check with
`CF_UI_UPDATE_BASELINES=1` to produce reviewable baseline candidates.

## Related files

- `coverage-manifest.json` — the authoritative list of workflow steps; drift
  between this file and `tests/integration-test.js` fails the check before
  any workflow runs.
- `tests/integration-test.js` — the Playwright workflow implementations.
- `tests/oscal-export-test.js`, `tests/sarif-export-test.js` — the Phase 5
  and Phase 6 export validation drivers.
- `baselines/` — committed strict-baseline PNGs; see
  `baselines/README.md` and `approve-baselines.sh` for the approval
  workflow. Do not hand-edit this directory outside that workflow.
- `design-parity/` — the offline design-parity rendering and comparison
  harness.
- `design-fixtures.json` — shared fixture data referenced by the harness.

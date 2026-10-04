---
type: Testing Guide
title: Crystal Forge flake checks catalog
description: Catalogs each flake check that has a README (integration, oidc-auth, web-ui, server-regressions, schema and metadata checks, and the dev-script checks) plus the cf-test-suite scenario runner, saying what each verifies, how to run it, and where its README lives.
tags:
  - crystal-forge
  - testing
  - flake-checks
  - nix
  - catalog
implementation_status: implemented
generated:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-03T22:56:18-05:00
sources:
  - id: integration
    resource: "Crystal Forge repository file checks/integration/README.md at commit 3b23d36f"
    title: Integration Check
  - id: nixos-options-metadata
    resource: "Crystal Forge repository file checks/nixos-options-metadata/README.md at commit 3b23d36f"
    title: NixOS Options Metadata Check
  - id: oidc-auth
    resource: "Crystal Forge repository file checks/oidc-auth/README.md at commit 3b23d36f"
    title: OIDC Authentication Check
  - id: oscal-export
    resource: "Crystal Forge repository file checks/oscal-export/README.md at commit 3b23d36f"
    title: OSCAL Export Check
  - id: run-ui-dev-db-check
    resource: "Crystal Forge repository file checks/run-ui-dev-db-check/README.md at commit 3b23d36f"
    title: run-ui-dev Database Behavior Check
  - id: server-regressions
    resource: "Crystal Forge repository file checks/server-regressions/README.md at commit 3b23d36f"
    title: Server Regressions Check
  - id: stig
    resource: "Crystal Forge repository file checks/stig/README.md at commit 3b23d36f"
    title: STIG Module Unit Tests
  - id: ui-screenshots
    resource: "Crystal Forge repository file checks/ui-screenshots/README.md at commit 3b23d36f"
    title: UI Screenshots Check
  - id: web-ui
    resource: "Crystal Forge repository file checks/web-ui/README.md at commit 3b23d36f"
    title: Web UI Check
  - id: web-ui-baselines
    resource: "Crystal Forge repository file checks/web-ui/baselines/README.md at commit 3b23d36f"
    title: Web UI Baselines
  - id: web-ui-reconciliation
    resource: "Crystal Forge repository file checks/web-ui-reconciliation/README.md at commit 3b23d36f"
    title: Web UI Reconciliation Check
  - id: web-ui-test-runner
    resource: "Crystal Forge repository file checks/web-ui-test-runner/README.md at commit 3b23d36f"
    title: Web UI Test Runner Check
  - id: xccdf-schema
    resource: "Crystal Forge repository file checks/xccdf-schema/README.md at commit 3b23d36f"
    title: XCCDF Schema Check
  - id: cf-test-suite
    resource: "Crystal Forge repository file packages/cf-test-suite/README.md at commit 3b23d36f"
    title: cf-scenarios (Crystal Forge test data)
---

# Crystal Forge flake checks catalog

This concept is a catalog and pointer record. Each check keeps its own
`README.md` next to its `default.nix`. Those README files are the
authoritative text and stay in place, because each check and package uses its
README as its ecosystem entry point. This catalog records, for each README, what
the check verifies, how to run it, and where the README lives. The summaries
come from reading each README together with the matching `default.nix` at the
migration base commit.

Every check below runs with
`nix build .#checks.x86_64-linux.<name> --print-build-logs`, unless the section
says otherwise. For the web UI check in depth, see the
[Web UI check runbook](web-ui-check.md). For the overall test strategy, see the
[Crystal Forge testing plan](test-plan.md).

## Summary

| Check | Boots a VM | Covers | README |
| --- | --- | --- | --- |
| `integration` | Yes (server and git server) | Server, database, and Grafana dashboard behavior through the `cf_test` pytest markers `database`, `dashboard`, and `server` | [README](../../../checks/integration/README.md) |
| `oidc-auth` | Yes (Keycloak and server) | OIDC login path against a real Keycloak realm | [README](../../../checks/oidc-auth/README.md) |
| `web-ui` | Yes (server, agent, builder, git server) | Production embedded-UI server through a real browser | [README](../../../checks/web-ui/README.md) |
| `web-ui-reconciliation` | Yes (nginx only) | One STIG import reconciliation workflow with mocked API routes | [README](../../../checks/web-ui-reconciliation/README.md) |
| `web-ui-test-runner` | No | The `web-ui-test` wrapper script logic | [README](../../../checks/web-ui-test-runner/README.md) |
| `ui-screenshots` | No | Fixture-driven screenshots of every view in two themes | [README](../../../checks/ui-screenshots/README.md) |
| `server-regressions` | No | PostgreSQL-backed Rust regression tests and a migration upgrade rehearsal | [README](../../../checks/server-regressions/README.md) |
| `oscal-export` | No | OSCAL 1.1.2 schema validation of fixture output | [README](../../../checks/oscal-export/README.md) |
| `xccdf-schema` | No | XCCDF 1.2 and `cf-xccdf-1` schema validation | [README](../../../checks/xccdf-schema/README.md) |
| `stig` | No | `mkStigModule` override and merge semantics | [README](../../../checks/stig/README.md) |
| `nixos-options-metadata` | No | Extracted NixOS option type metadata | [README](../../../checks/nixos-options-metadata/README.md) |
| `run-ui-dev-db-check` | No | `run-ui-dev` database probe and start scripts | [README](../../../checks/run-ui-dev-db-check/README.md) |

## Server and database checks

### `integration`

README: [checks/integration/README.md](../../../checks/integration/README.md).

- **What it verifies.** The check boots a NixOS VM with the Crystal Forge
  server, its embedded PostgreSQL database, an agent, and Grafana, plus a
  second VM that serves a real Git repository the server polls as a watched
  flake. The test script runs the `cf_test` pytest markers `database`,
  `dashboard`, and `server` in that order, and stops at the first failing
  phase. It also asserts the server's systemd resource-control settings
  (`MemoryHigh`, `MemoryMax`, `MemorySwapMax`, slice assignment), Grafana
  health, and the provisioned PostgreSQL datasource.
- **Scope limits.** The builder is disabled (`build.enable = false`) so server
  state transition tests do not interleave with real builds. The server uses
  the core build `cf-server-core-drv`, so the check never serves the web UI. It
  does not exercise OIDC.
- **Run it.** `nix build .#checks.x86_64-linux.integration --print-build-logs`.
  The global timeout in `default.nix` is 1200 seconds.
- **CI.** Listed in the `.gitlab-ci.yml` `flake-check` matrix.

### `oidc-auth`

README: [checks/oidc-auth/README.md](../../../checks/oidc-auth/README.md).

- **What it verifies.** The check boots a Keycloak VM with an imported
  `crystal-forge` realm and a Crystal Forge server VM with
  `auth_mode = "oidc"`. It asserts that the discovery document is reachable
  directly and from the server node, that the password-grant token exchange
  returns access, ID, and refresh tokens, that realm roles appear in the
  `roles` claim with `admin`, that the server reports `auth_mode: "oidc"` and an
  unauthenticated `whoami`, that `/api/auth/oidc/login` redirects to Keycloak,
  and that the `users`, `user_sessions`, and `external_identities` tables exist.
- **Scope limits.** No browser is involved, the builder is disabled, and the
  server uses `cf-server-core-drv`. The realm export is
  `checks/oidc-auth/realm-crystal-forge.json`.
- **Run it.** `nix build .#checks.x86_64-linux.oidc-auth --print-build-logs`.
  The global timeout in `default.nix` is 600 seconds.
- **CI.** Listed in the `.gitlab-ci.yml` `flake-check` matrix.

### `server-regressions`

README: [checks/server-regressions/README.md](../../../checks/server-regressions/README.md).

- **What it verifies.** The check builds `cf-server` test targets with
  `buildRustPackage` and runs them against a disposable PostgreSQL instance from
  `postgresqlTestHook`. It first upgrades a populated database that has only
  migrations through `0232` to the full migration set and checks that every
  populated row family survives. It then runs a curated list of `cf-server`
  Cargo integration test binaries and a curated list of `#[ignore]`d library
  tests (POA&M, policy, notification, composite AC3, STIG mapping, bundle
  baseline, and agent key rotation areas).
- **Why it exists.** `nix build .#server` runs only `--lib --bins` tests, and
  the `integration` check runs the Python suite, so these Rust integration
  targets would otherwise run nowhere. The list is curated on purpose and is
  not `--all-targets --ignored`.
- **Run it.**
  `nix build .#checks.x86_64-linux.server-regressions --print-build-logs`. No VM
  boots. `SQLX_OFFLINE=true` is set, and the test role has
  `LOGIN SUPERUSER CREATEDB`.
- **CI.** Listed in the `.gitlab-ci.yml` `flake-check` matrix.

> **Status:** the README test list is slightly shorter than the code. The
> `cargo test` invocation in `checks/server-regressions/default.nix` also runs
> the `compliance_assignment_zombie_repair` test binary, which the README list
> does not name.

## Web UI checks

### `web-ui`

README: [checks/web-ui/README.md](../../../checks/web-ui/README.md). The
procedural guide is the [Web UI check runbook](web-ui-check.md).

- **What it verifies.** This is the authoritative pre-merge gate for the web UI.
  It boots a NixOS VM with the production embedded-UI server build
  (`cf-server-drv`, not the core build), the agent, the builder, and a Git
  server, then drives the real UI with Playwright against the workflows in
  `checks/web-ui/coverage-manifest.json`. Gates are: build verification of the
  served `index.html`, JS loader, and WASM magic header; a coverage gate that
  fails when `tests/integration-test.js` and the manifest disagree; a list of
  critical workflows (`critical_tests` in `default.nix`) that must be present
  and pass; strict visual baselines; OSCAL export validation (Phase 5) and SARIF
  export validation (Phase 6) of real browser downloads; and a non-blocking
  design-parity comparison against the design example.
- **Do not change.** The README forbids rebinding the server package to the core
  build, because this check proves that the shipped server binary serves the
  shipped WASM bundle.
- **Run it.** `nix build .#checks.x86_64-linux.web-ui --print-build-logs`. To run
  a subset, set `CF_UI_TEST_STEPS` to a comma-separated workflow list and add
  `--impure`. Other `--impure` variables are `CF_UI_TEST_PROFILE` (default
  `ci_fast`), `CF_UI_UPDATE_BASELINES=1`, and `CF_WEB_UI_RUN_MEGA_PHASES=1`
  (interactive only; boots Attic and S3 cache VMs and runs legacy pytest
  phases).
- **CI.** The README says the check is in the `flake-check` matrix. See the
  status note below.

> **Status:** timeout and CI statements in the README differ from the code. The
> README says the full manifest has a 2400-second global timeout and a
> 1800-second `playwrightResultTimeout` default. At the migration base commit,
> `checks/web-ui/default.nix` sets `globalTimeout = 3000` and
> `playwrightResultTimeout ? 2700`. The README also says `web-ui` is in the
> `flake-check` matrix, but `.gitlab-ci.yml` lists it as a separate
> `web-ui-check` job with `allow_failure: true`, and the matrix names are
> `integration`, `oidc-auth`, `run-ui-dev-db-check`, `server-regressions`, and
> `web-ui-test-runner`. A later verification pass must reconcile these
> statements.

### `web-ui/baselines`

README: [checks/web-ui/baselines/README.md](../../../checks/web-ui/baselines/README.md).

This directory holds reviewed visual baselines for `strict` manifest steps. It
is not a check. Generate captures through the `web-ui` check and approve them
with `checks/web-ui/approve-baselines.sh`, then commit the PNG files. The
README forbids adding failed-step diagnostics, diff images, reports, or
export-test screenshots to this directory. The approval procedure is in the
[Web UI check runbook](web-ui-check.md).

### `web-ui-reconciliation`

README: [checks/web-ui-reconciliation/README.md](../../../checks/web-ui-reconciliation/README.md).

- **What it verifies.** One VM runs nginx and serves only the static production
  web UI build, with no server, database, or backend. The check asserts that
  the served `index.html`, JS loader, and WASM magic header are valid, and then
  runs the single workflow `20ac-stig-import-reconciliation-fixture` with
  `CF_UI_TEST_STANDALONE=1`, so Playwright route mocks supply every API
  response. It requires light and dark screenshots.
- **Why it exists.** It reuses `checks/web-ui/tests/integration-test.js` and
  `checks/web-ui/coverage-manifest.json`, so one workflow can run without the
  cost of the full `web-ui` VM.
- **Scope limits.** Only that one workflow runs. A workflow that needs a real
  backend response cannot run here.
- **Run it.**
  `nix build .#checks.x86_64-linux.web-ui-reconciliation --print-build-logs`.
- **CI.** Not named in the `flake-check` matrix. It runs under a broader
  `nix flake check`.

### `web-ui-test-runner`

README: [checks/web-ui-test-runner/README.md](../../../checks/web-ui-test-runner/README.md).

- **What it verifies.** The check tests the host-side `web-ui-test` wrapper
  (`checks/web-ui/tests/web-ui-test.sh`) with
  `checks/web-ui/tests/web-ui-test-runner-test.sh`. The tested contract is
  workflow selection, rejection of workflows that are not listed in
  `settings.devStackWorkflows` in `coverage-manifest.json`, development stack
  readiness reporting, artifact creation, and exit-status propagation. No
  Playwright session, service, or VM runs.
- **Run it.**
  `nix build .#checks.x86_64-linux.web-ui-test-runner --print-build-logs`.
- **CI.** Listed in the `.gitlab-ci.yml` `flake-check` matrix.

### `ui-screenshots`

README: [checks/ui-screenshots/README.md](../../../checks/ui-screenshots/README.md).

- **What it verifies.** `capture.js` drives Playwright against the served
  production WASM bundle, intercepts every `/api/v1/` call with JSON built from
  `docs/design/CrystalForge/fixtures/crystal-forge.fixtures.json`, and writes one
  PNG per view and theme. `default.nix` describes 13 views in 2 themes, which is
  26 PNGs. It makes no semantic assertions and runs no interactive workflow. It
  needs no backend, database, or network.
- **Run it.** `nix build .#checks.x86_64-linux.ui-screenshots --print-build-logs`
  or `nix build .#ui-screenshots` (the same derivation exposed as a package in
  `flake.nix`). The derivation uses `__noChroot = true` for the bundled Chromium.
- **CI.** Not named in the `flake-check` matrix.
- **Related files.** `capture.js`, `routes.js`, `generate-fixture-routes.js`,
  `seed-db.spec.ts`, `playwright.config.ts`, and `tsconfig.json` in the same
  directory.

The [golden fixtures contract](design-golden-fixtures.md) describes the fixture
data this check reads.

## Data-contract and unit checks (no VM)

### `oscal-export`

README: [checks/oscal-export/README.md](../../../checks/oscal-export/README.md).

- **What it verifies.** `crystal-forge-oscal-fixture` produces a deterministic
  OSCAL Assessment Results document. `validate.py` validates that document and
  the Assessment Plan and System Security Plan documents it chains to
  (AR, AP, SSP) against the vendored NIST OSCAL 1.1.2 JSON schemas
  (`pkgs.crystal-forge.oscal-1-1-2-schemas`) with `jsonschema` and `regex`.
- **Scope limits.** The check does not exercise the export modal or the
  production WASM `build_oscal()` path. The `web-ui` check's Phase 5 covers that.
- **Run it.** `nix build .#checks.x86_64-linux.oscal-export --print-build-logs`.
- **CI.** Not named in the `flake-check` matrix.

### `xccdf-schema`

README: [checks/xccdf-schema/README.md](../../../checks/xccdf-schema/README.md).

- **What it verifies.** The check validates the vendored XCCDF 1.2 schema and
  the repository's `cf-xccdf-1` extension schema (`schemas/cf-xccdf-1/`). It uses
  hand-authored fixtures and the real output of the `xccdf-export-fixture`
  binary. `xmllint` performs schema validation, a `check-content/policy` element
  count is asserted, each extracted CF extension node type is validated against
  the CF extension schema, and OpenSCAP (`oscap xccdf validate`, `oscap info`)
  also accepts the hand-authored and writer-generated documents.
- **Scope limits.** The check does not exercise the HTTP export endpoint,
  authentication, or authorization. The fixture binary comes from
  `cf-server-core-drv`.
- **Run it.** `nix build .#checks.x86_64-linux.xccdf-schema --print-build-logs`.
- **CI.** Not named in the `flake-check` matrix.

### `stig`

README: [checks/stig/README.md](../../../checks/stig/README.md).

- **What it verifies.** A pure Nix evaluation tests
  `lib.crystal-forge.mkStigModule`, the helper that sets NixOS option values at a
  priority that wins over ordinary module definitions. The README lists nine
  tests: a plain STIG value beats an ordinary definition; STIG `mkForce` beats a
  user `mkForce`; a bare `mkBefore` keeps its ordering; a nested
  `mkDefault (mkBefore ...)` wrapper is unwrapped and rewrapped; attribute-set
  override wrappers (the AIDE pattern) keep every key; a full `evalModules` run
  reproduces and closes the TASK-398 crash; `mkIf` keeps its condition; and
  `mkMerge` merges at leaf and whole-`stigConfig` level.
- **Scope limits.** The check exercises only the `mkStigModule` mechanism against
  a minimal scaffold module. It does not test any real STIG control module.
- **Run it.** `nix build .#checks.x86_64-linux.stig --print-build-logs`.
- **CI.** Not named in the `flake-check` matrix.

> **Status:** the README says nine tests. `checks/stig/default.nix` at the
> migration base commit also defines `t10` (distinct controls own distinct
> tracking options) and `t11` (identical control names remain declaration
> conflicts), so the code has more tests than the README names.

### `nixos-options-metadata`

README: [checks/nixos-options-metadata/README.md](../../../checks/nixos-options-metadata/README.md).

- **What it verifies.** A pure Nix evaluation over the `nixos-options-metadata`
  package asserts that the metadata list is sorted by option path, that five
  known options resolve to the expected `value_type`
  (`networking.firewall.enable` is `boolean`, `networking.networkmanager.dns` is
  a non-empty `enum`, `boot.consoleLogLevel` is `integer`, `networking.hostName`
  is `string`, `networking.extraHosts` is `lines`), and that
  `share/crystal-forge/nixos-options.json` exists and is not empty.
- **Scope limits.** The check does not verify how the server or web UI consume
  the metadata.
- **Run it.**
  `nix build .#checks.x86_64-linux.nixos-options-metadata --print-build-logs`.
- **CI.** Not named in the `flake-check` matrix.

## Developer tooling checks

### `run-ui-dev-db-check`

README: [checks/run-ui-dev-db-check/README.md](../../../checks/run-ui-dev-db-check/README.md).

- **What it verifies.** The check runs two bash tests against the real scripts
  in `packages/devScripts/`. `db-usability-check.sh` must tell apart a
  PostgreSQL process that merely answers on a port from one where the
  `crystal_forge` role can use the `crystal_forge` database and the process
  belongs to the current worktree (the test mocks `psql` and uses real `ss` and
  `/proc` lookups against background processes it spawns).
  `db-only-start.sh` must start the db-only service with `$PROJECT_ROOT` as its
  working directory, so the relative `dataDir` (`./data/db`) resolves where the
  worktree-identity check expects.
- **Run it.**
  `nix build .#checks.x86_64-linux.run-ui-dev-db-check --print-build-logs`. No VM
  and no real PostgreSQL run.
- **CI.** Listed in the `.gitlab-ci.yml` `flake-check` matrix.

The local development stack that these scripts support is described in the
[fixture seeding developer guide](fixture-seeding.md).

## Scenario runner (`packages/cf-test-suite`)

README: [packages/cf-test-suite/README.md](../../../packages/cf-test-suite/README.md).

This README is not a flake check. It documents `cf-scenarios`, the command that
populates a Crystal Forge database with preset test scenarios. Its content is a
help command, a quick start, and examples:

- `nix run .#cf-test-modules.scenarioRunner -- -h` prints help.
- `-s <scenario>` selects a scenario, for example `up_to_date`,
  `mixed_commit_lag`, `flake_time_series`, `behind`, `flaky_agent`,
  `latest_with_two_overdue`, `never_seen`, `agent_restart`, `rollback`, and
  `offline`.
- Common knobs are `--num-systems`, `--agent-version`, `--num-overdue`,
  `--ok-heartbeat-minutes`, `--overdue-minutes`, `--heartbeat-interval-minutes`,
  `--heartbeat-hours`, `--base-hostname`, `--stagger-window-minutes`, and
  `--hostname`. A generic `--param key=value` passes scenario-specific keyword
  arguments.
- Database connection overrides use `DB_HOST`, `DB_PORT`, `DB_USER`,
  `DB_PASSWORD`, and `DB_NAME`.

> **Status:** the README's `.#cf-test-modules.scenarioRunner` attribute name does
> not match the repository at the migration base commit. The package directory
> is `packages/cf-test-suite/`, the Nix attribute is
> `pkgs.crystal-forge.cf-test-suite` (with a `scenarioRunner` member that builds
> `cf-scenarios`), and the development shell alias `run-db-test` runs
> `.#cf-test-suite.runTests`. The test plan uses the same old `cf-test-modules`
> name (see [the testing plan](test-plan.md)). A later verification pass must
> confirm the correct `nix run` attribute path.

## Checks without a README

At the migration base commit, these directories under `checks/` have no
README, and this catalog does not describe them: `builder-cve-contract`,
`builder-evaluator-packaging`, `config-inspector`, `config-observer`,
`evaluator-snapshot-isolation`, `okf-knowledge`, `test-keys`, and
`verified-source-evaluator-parity`. Read each `default.nix` for its scope.

## Related concepts

- [Web UI check runbook](web-ui-check.md)
- [Crystal Forge testing plan](test-plan.md)
- [Fixture seeding developer guide](fixture-seeding.md)
- [Golden fixtures contract](design-golden-fixtures.md)
- [Offline flake input prefetching for NixOS VM tests](offline-flake-prefetch.md)

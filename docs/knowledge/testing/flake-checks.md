---
type: Testing Guide
title: Crystal Forge flake checks catalog
description: "Catalogs integration owners, consolidated component verification, and historical retired suites, preserving provenance and distinguishing execution evidence from coverage gaps."
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

## Current inventory and commands

TASK-490.24 authorizes source retirement before four-owner completion. The
current `checks.x86_64-linux` inventory contains `integration-access` and
`integration-delivery` (two total attributes, previously 27). Access's bootstrap,
session, authorization and Admin-form subset does not complete Access. Delivery
has a real source/evaluation/build/agent lifecycle, not complete Delivery coverage.
Assurance and Upgrade are absent. No placeholders
are registered, including beneath the retained Assurance fixture directory.

The final inventory must contain exactly four TOTAL names. Verify the full set:

```bash
nix eval --json .#checks.x86_64-linux --apply builtins.attrNames
nix eval --json .#checks.x86_64-linux --apply 'c: builtins.attrNames c == [ "integration-access" "integration-assurance" "integration-delivery" "integration-upgrade" ]'
```

Final acceptance requires `true`, complete accepted coverage, and passing real
aggregates. See the [contract](integration-test-contract.md) and
[retirement map](integration-coverage-map.md#legacy-retirement-map).

The initial one-for-one package relocation is superseded. All fifteen public
`verify-*` definitions are removed. Unique assertions use the following existing
component homes. This table documents ownership; it is not an execution registry.

| Former verification name | Current source and primary execution |
| --- | --- |
| `config-inspector` | [config-inspection.nix](../../../packages/default/crates/cf-server/tests/nix/config-inspection.nix), normal core server `postCheck` |
| `config-observer` | [config-observation.nix](../../../packages/default/crates/cf-server/tests/nix/config-observation.nix), normal core server `postCheck` |
| `evaluator-snapshot-isolation` | [primary-isolation.nix](../../../packages/default/crates/cf-server/tests/nix/primary-isolation.nix), normal core server `postCheck` |
| `verified-source-evaluator-parity` | [source-parity.nix](../../../packages/default/crates/cf-server/tests/nix/source-parity.nix), normal core server `postCheck`, with its locked verified-source fixture |
| `niks3-ca-bundle` | All thirteen unchanged `cf-config` library tests in the normal core build; [compatibility.nix](../../../packages/default/crates/cf-config/test-support/ca-bundle/compatibility.nix) compares Go/Rust full-DER identities on three datasets in core `postCheck` |
| `server-regressions` | Existing `devScripts.state-machine-test`; [database-regressions.sh](../../../packages/devScripts/tests/database-regressions.sh) and [state-machine-tests.sh](../../../packages/devScripts/tests/state-machine-tests.sh) |
| `builder-cve-contract` | [builder-cve.nix](../../../modules/nixos/crystal-forge/tests/builder-cve.nix), explicit boolean Nix evaluation |
| `stig` | [stig.nix](../../../modules/nixos/crystal-forge/tests/stig.nix), explicit boolean Nix evaluation |
| `builder-evaluator-packaging` | [packaging.nix](../../../modules/nixos/crystal-forge/tests/packaging.nix), pure module evaluation; [packaging.sh](../../../modules/nixos/crystal-forge/tests/packaging.sh), installed-wrapper and generated-service probes |
| `nixos-discovery-reliability` | [discovery.sh](../../../modules/nixos/crystal-forge/tests/discovery.sh) and [discovery.py](../../../modules/nixos/crystal-forge/tests/discovery.py), direct runtime test with independent temporary Nix stores |
| `nixos-options-metadata` | Ordinary [nixos-options-metadata](../../../packages/nixos-options-metadata/default.nix) assertions and `postCheck` |
| `oscal-export` | Ordinary [oscal-fixture](../../../packages/oscal-fixture/default.nix) check phase, validating the NIST AR/AP/SSP chain with [validate.py](../../../packages/oscal-fixture/validate.py) |
| `run-ui-dev-db-check` | Existing [DB usability selftest](../../../packages/devScripts/db-usability-check-test.sh) and [DB start selftest](../../../packages/devScripts/db-only-start-test.sh), direct CI commands under `nix develop` |
| `web-ui-test-runner` | Existing [host runner selftest](../../../packages/devScripts/web-ui/tests/web-ui-test-runner-test.sh), direct CI command under `nix develop` |
| `xccdf-schema` | [xccdf-schema.sh](../../../packages/default/crates/cf-server/tests/xccdf-schema.sh), installed real exporter fixture XML, CF extensions, and OpenSCAP validation in direct component CI |

The four evaluator fragments run once in the normal core server build, not in
the embedded build. Existing Rust test definitions remain. Core explicitly
selects `--package cf-config` as well as `--package cf-server --lib --bins`;
selecting only `cf-server` did not run dependency-library CA tests. The separate
CA runner and fixture derivations are removed. Dataset comparison reduces Go
invocations from nine to three and OpenSSL parses from 1,758 to 879 without
removing the cross-language identity assertions.

One explicit `component-tests` CI job replaces the six-entry
`package-verification` matrix. It builds core, metadata, OSCAL and documentation,
then runs module evaluations, discovery, installed-wrapper/schema tests and
developer-tool selftests directly. See [.gitlab-ci.yml](../../../.gitlab-ci.yml)
and the [verification guide](../../agents/verification.md#consolidated-component-verification)
for commands. `.#okf-knowledge` and `.#ui-screenshots` remain ordinary packages;
mocked screenshots provide design feedback, not semantic proof.

The three database jobs remain. Their existing apps share the state-machine
invocation-owned launcher and PostgreSQL module. CVE uses port 35432,
state-machine uses 35433, and dashboard uses 35434. Each invocation has fresh
temporary data and short socket directories, `LOGIN SUPERUSER CREATEDB`, and
`pg_stat_statements` preload. PostgreSQL children alone drop privileges on root
runners. The SQLx shell verifies data-directory ownership before migrations and
exports both `DATABASE_URL` and `CRYSTAL_FORGE_TEST_DATABASE_URL`; all selections
share `target/database-tests`. The state-machine source retains its selected
ignored library regressions and fourteen integration targets. Its current
execution count requires a fresh normal run. Two library selections cover TASK-496
heartbeat/state decoding. Six repeated ignored selections and
94 nonignored library selections are removed; normal package `--lib --bins`
owns the latter. The whole `composite_policy` target owns two tests removed from
the CVE job. Both populated migration rehearsals and exact missing-test guards
remain. CVE retains its 27 workspace library filters, selecting 27 nonignored
and three ignored tests. Dashboard retains both ignored visibility prefixes,
selecting four tests. The dashboard query prefix owns
`visibility_scope_handles_ambiguous_systems_cache_pushes_and_eval_attempts`;
TASK-501 removes that one duplicate from the consolidated runner. The other
dedicated selections do not overlap its database selections. Neither dedicated
job is fully subsumed, so both remain. Dashboard runs both groups and retains
any failure.

All three apps preserve the actual Cargo status and bind actual emitted native
libtest names to the listed executable selection. The guard preserves Cargo
targets/filters and libtest ignored/exact options, subtracts ignored-only names
from default listings, rejects empty executable selections, and compares name
multisets and result counts. Listing does not execute test bodies. Unrelated
positive results with the same count fail; no static registry is used.
The launcher rejects absent/invalid completion receipts even if process-compose
returns zero. Cleanup waits for PostgreSQL before removing invocation storage
and preserves earlier errors. CI prepares closures/Cargo sources explicitly;
outer execution, nested SQLx shells, and descendant Nix commands disable remote
builders, substitution and fetch fallback. See the
[database commands and failure-path regression](../../agents/verification.md#database-ci-failure-path-regression).
The dedicated jobs run the direct Python regression with temporary fault
configurations and retain `database-ci-evidence/`. That controlled evidence does
not count as product-test execution or add a flake check.

Initial TASK-501 local execution at `dbf50b47` plus the runner changes exposed failing
assertions: CVE returned 101 after one passing and one failing test; normal
dashboard execution returned 101 with three passing and one failing test on
both root/nonroot paths. The failing tests are
`recorded_derivation_path_restores_when_output_deriver_is_unknown` and
`visibility_deployment_status_preserves_ahead_as_up_to_date`. Rust code and
assertions were unchanged at the runner-only checkpoint `ee32c209`. Separate selection diagnostics and controlled
failure-path checks must not be reported as a passing full CVE/dashboard job.
Selection inventory is not execution evidence. The task notes retain exact
names, counts, statuses, source and raw evidence locations.

The authorized follow-up corrects only four `cfg(test)` fixtures under the
existing source contracts: one eligible destination for legacy cache
materialization, an authoritative successful post-build job, complete retention
of the known authorized diagnostic-event baseline after stale completion, and
a same-flake deployable target for ahead classification. The expired-lease
denial now compares every persisted event field before/after the rejected call;
it does not assume that the earlier valid `scanner_started` event disappeared.
Production logic and original semantic/side-effect denial assertions are
preserved. Test names and runner selections are unchanged.

Normal prepared offline CVE/dashboard entries now executed successfully on both
root-namespace and nonroot paths: CVE 30 passed/0 failed/0 ignored, dashboard
4 passed/0 failed/0 ignored, each exit 0. Every output name and per-selection
mode/count matched the existing inventory, including the three explicitly
enabled ignored CVE tests. This is local normal-entry evidence, distinct from
the earlier failing checkpoint and its separate selector diagnostics. See the
[fixture follow-up record](../../agents/verification.md#authorized-fixture-follow-up-to-ee32c209)
for source pins, retained failed environmental attempt, raw logs, formatting
limitations and remaining coordinator-owned verification.

The subsequent review correction strengthens the existing CI regression's
cleanup evidence: nonempty reported data/socket sets, actual `[db]` postmaster
PID and socket identity, same-PID final shutdown, and `/proc` starttime/death
corroboration where readable. Temporary `db-init` shutdown is not accepted.
The repeated root/nonroot matrix passed 32 controlled process cases, including
unrelated positive names, and rejected eight mutated evidence fixtures missing
the main shutdown or path report. Normal root CVE/dashboard entries passed
30/4 unchanged test names with runtime membership checks. Focused broad mixed
library/expected-panic/exact ignored integration checks also passed; the full
state-machine run remains unverified locally. See the
[runtime evidence record](../../agents/verification.md#runtime-membership-and-main-server-evidence-follow-up).

Removed verification derivations total eighteen: fifteen public roots, the
nested module-validation derivation, and the CA fixture and runner. Twenty-two
expression helpers and six fixture generators remain unique component inputs,
not gates. The measured recursive derivation union for core plus the former
five-package slice fell from 5,428 to 4,432 (996 fewer). This is a dependency-DAG
measurement, not a measured runtime improvement.

Handwritten XCCDF examples are retired static examples, not production test
evidence. The direct schema command validates actual writer output and all five
CF extension node types with non-vacuous extraction. Rust tests retain custom
check and structural semantics; this does not prove XSD validity of every
custom-check variant. Full current core/database verification and newly composed
Admin-form execution require their own successful results; the recorded sixteen
Access cases remain the only executed integration proof.

The nine legacy integration suite directories are retired. CI jobs
`legacy-flake-check`, `web-ui-check`, and `web-ui-baseline-candidates` are
removed. Useful browser developer assets live in `packages/devScripts/web-ui/`.
Both XCCDF XML revisions live in `checks/integration-assurance/fixtures/xccdf/`.
Unused `test-keys` placeholders are removed. These actions do not establish
replacement integration coverage or turn legacy implementation details into
mandatory requirements.

## Historical catalog (before source retirement)

The remainder preserves the migration-base catalog and subsequent historical
scope notes. Old `checks` commands, CI memberships, paths and baseline procedures
below describe retired or relocated implementations and are not current run
instructions. Use the component table above for retained verification. Historical
README links for retired suites resolve to the last retained source revision;
successor source links identify the consolidated implementation.

This concept was a catalog and pointer record. Each check kept its own
`README.md` next to its `default.nix`. The removed READMEs remain historical
provenance; current execution ownership is defined above. This catalog records what
the check verifies, how to run it, and where the README lives. The summaries
come from reading each README together with the matching `default.nix` at the
migration base commit. The TASK-470 packaging and Niks3 sections and cache
regression summary were added after rebase and describe the current checks,
not files or behavior present at the original migration base `3b23d36f`.

Every check below runs with
`nix build .#checks.x86_64-linux.<name> --print-build-logs`, unless the section
says otherwise.

The integration layer is being replaced by four owners (`integration-access`,
`integration-delivery`, `integration-assurance`, `integration-upgrade`) under
the [integration test maintenance contract](integration-test-contract.md). Only
`integration-access` is currently discoverable. Its coordinator-owned
`checks/integration-access/default.nix` explicitly registers scenarios, and its
normal `pkgs.linkFarm` check builds every registered scenario gate. Meaningful
WIP checks may be discoverable without a completeness flag; missing obligations
remain in the [coverage map](integration-coverage-map.md), which also
gives each legacy check below its retirement disposition. For the web UI check in depth, see the
[Web UI check runbook](web-ui-check.md). For the overall test strategy, see the
[Crystal Forge testing plan](test-plan.md).

Shared helpers live in `lib/integration-test/`; native Snowfall checks call
`lib.crystal-forge.makeIntegrationTestHarness { inherit pkgs inputs; }`. A
packet does not automatically get a NixOS derivation. Compatible packet cases
are fragments composed and registered by the coordinator using the existing
Nix/Python/browser-script structure. A separately built scenario gets a fresh
VM/database; same-scenario cases may use receipts from earlier real workflows.
Future Delivery, Assurance, and Upgrade scenarios, fixtures, and assets belong
beneath `checks/integration-{delivery,assurance,upgrade}/`. Do not create
placeholder `default.nix` files or custom discovery.

GitLab runs each implemented owner automatically: the `integration-access`
job builds the normal owner aggregate and is required on merge requests and
`main`. The retiring legacy integration checks stay defined and playable as
optional manual jobs: `integration`, `oidc-auth`, `niks3-cache`,
`niks3-write-probe`, `niks3-ui`, `attic-metrics`, and `attic-publication` run
as `legacy-flake-check: [<name>]`, and `web-ui` runs as `web-ui-check`. Removing a legacy
check from automatic CI does not mark its obligations covered. Delete its
definition and fixtures after passing replacement coverage is recorded, or
after an explicit decision removes the underlying requirement. The
[coverage map](integration-coverage-map.md#current-gitlab-selection) lists the
automatic coverage gaps.

Run the focused Access gate with
`nix build .#checks.x86_64-linux.integration-access.scenarios.lifecycle-local-bootstrap -L`.
Append `.run` to the scenario attribute for evidence only. The historical
scenario at `fc930335878279ac426510359c039dfd8961dc13` failed before
migrations/browser/auth. TASK-490.25/.26 repairs are integrated. After rebase on
`origin/dev` `5ae7feab`, the latest focused scenario passed ten cases at
`81d65be745cc9f9777d368900d01fe0f207cb052`: five unchanged bootstrap cases
and five owner-selected session/CSRF/sign-out cases. The command
`nix build .#checks.x86_64-linux.integration-access -L` also passed, reusing
that run. This verifies bootstrap and the selected session proof, not full
Access coverage or the other owners. The same VM journal records unrelated
TASK-492. See the
[foundation execution record](integration-coverage-map.md#foundation-execution-record)
for the generated configuration, exact output path, expected/actual result,
and task links. TASK-491's package test prerequisite is resolved. The CI job
script `sh packages/ci/integration-owner-build.sh integration-access` also
executed the VM and passed the same ten cases at
`2c494ac4bf8cc7155899723ee69fcf879ac49248`, and all 16 cases (adding six
Operator/Viewer authorization cases) at `8a857bc872cf331a19036b97a93f2b7741e9728b`;
it exports real evidence files to `integration-evidence/integration-access/`,
including after a failed build, and keeps the build status if collection fails.
The former retain-until-cutover instruction is superseded by the authorized
source retirement. This historical result does not restore a legacy gate.

## Summary

Only Access and Delivery are current integration checks. The other rows retain
historical suite inventories or identify current lower-level/tool successors;
they are not additional flake check attributes.

| Check | Boots a VM | Covers | README |
| --- | --- | --- | --- |
| `integration-access` | Yes (one shared controller; automatic and required) | Bootstrap/migrations, local sign-in/reload, sessions/CSRF/sign-out, selected Admin-only role denials and Admin form persistence. Remaining environment, registration and other Access obligations stay gaps. | [Coverage map](integration-coverage-map.md) |
| `integration-delivery` | Yes (one five-role lifecycle; automatic and required) | Authored source/native evaluation, exact derivation/attempt, two API-only builder sessions, cold real Nix output, signed local publication and target-agent reporting. Remaining Delivery obligations stay gaps. | [README](../../../checks/integration-delivery/README.md) |
| `integration` | Yes (server and git server) | Server, database, and Grafana dashboard behavior through the `cf_test` pytest markers `database`, `dashboard`, and `server` | [Historical README](https://gitlab.com/crystal-forge/crystal-forge/-/blob/5fa1f4fa/checks/integration/README.md) |
| `oidc-auth` | Yes (Keycloak and server) | OIDC login path against a real Keycloak realm | [Historical README](https://gitlab.com/crystal-forge/crystal-forge/-/blob/5fa1f4fa/checks/oidc-auth/README.md) |
| `web-ui` | Yes (server, agent, builder, git server) | Production embedded-UI server through a real browser | [Historical README](https://gitlab.com/crystal-forge/crystal-forge/-/blob/5fa1f4fa/checks/web-ui/README.md) |
| `web-ui-reconciliation` | Yes (nginx only) | One STIG import reconciliation workflow with mocked API routes | [Historical README](https://gitlab.com/crystal-forge/crystal-forge/-/blob/5fa1f4fa/checks/web-ui-reconciliation/README.md) |
| `web-ui-test-runner` | No | The `web-ui-test` wrapper script logic | [Successor selftest](../../../packages/devScripts/web-ui/tests/web-ui-test-runner-test.sh) |
| `ui-screenshots` | No | Fixture-driven screenshots of every view in two themes | [README](../../../packages/ui-screenshots/README.md) |
| `server-regressions` | No | PostgreSQL-backed Rust regression tests and a migration upgrade rehearsal | [Successor runner](../../../packages/devScripts/tests/database-regressions.sh) |
| `builder-evaluator-packaging` | No | Evaluator-bound Nix/Niks3 wrappers, service PATH, proxy assertions, and generated configuration | [Successor runtime probes](../../../modules/nixos/crystal-forge/tests/packaging.sh) |
| `niks3-cache` | Yes (cache, server, builder, agent) | Real cache publication, proxy claims, capability gates, agent pulls, and local CVE materialization | [Historical README](https://gitlab.com/crystal-forge/crystal-forge/-/blob/5fa1f4fa/checks/niks3-cache/README.md) |
| `okf-knowledge` | No | OKF structural validation, checker unit tests, exact diagram audit, and a pinned Mermaid syntax parse of every diagram (no browser render; `nix run .#okf-mermaid-renderer` renders SVG manually) | — |
| `oscal-export` | No | OSCAL 1.1.2 schema validation of fixture output | [Successor validator](../../../packages/oscal-fixture/validate.py) |
| `xccdf-schema` | No | XCCDF 1.2 and `cf-xccdf-1` schema validation | [Successor script](../../../packages/default/crates/cf-server/tests/xccdf-schema.sh) |
| `stig` | No | `mkStigModule` override and merge semantics | [Successor assertions](../../../modules/nixos/crystal-forge/tests/stig.nix) |
| `nixos-options-metadata` | No | Extracted NixOS option type metadata | [Successor package tests](../../../packages/nixos-options-metadata/default.nix) |
| `run-ui-dev-db-check` | No | `run-ui-dev` database probe and start scripts | [Successor selftest](../../../packages/devScripts/db-usability-check-test.sh) |

## Server and database checks

### `integration`

Historical README: [checks/integration/README.md](https://gitlab.com/crystal-forge/crystal-forge/-/blob/5fa1f4fa/checks/integration/README.md).

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
- **CI.** Optional manual `legacy-flake-check: [integration]` job
  (`allow_failure: true`) in `.gitlab-ci.yml`. It does not run automatically.
  Its obligations are not covered by `integration-access`.

### `oidc-auth`

Historical README: [checks/oidc-auth/README.md](https://gitlab.com/crystal-forge/crystal-forge/-/blob/5fa1f4fa/checks/oidc-auth/README.md).

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
- **CI.** Optional manual `legacy-flake-check: [oidc-auth]` job
  (`allow_failure: true`) in `.gitlab-ci.yml`. It does not run automatically.
  Its obligations are not covered by `integration-access`.

### `server-regressions`

Current successor: [consolidated database runner](../../../packages/devScripts/tests/database-regressions.sh). The description below records the historical package/check, not the current launcher or test counts.

- **What it verifies.** The check builds `cf-server` test targets with
  `buildRustPackage` and runs them against a disposable PostgreSQL instance from
  `postgresqlTestHook`. It first upgrades a populated database that has only
  migrations through `0232` to the full migration set and checks that every
  populated row family survives. It then runs a curated list of `cf-server`
  Cargo integration test binaries and a curated list of `#[ignore]`d library
  tests (POA&M, policy, notification, composite AC3, STIG mapping, bundle
  baseline, and agent key rotation areas).
   The curated exact-name lists also run 52 ignored PostgreSQL tests and 51 named
  non-ignored tests for cache scope, retained credentials, signed capabilities,
  direct-peer dispatch, and publication-backed reads. Each invocation uses
  `--exact --test-threads=1` and requires the named success line and exactly one
   passed test with zero ignored tests. Only the 52 selected database tests use
  `--ignored`; missing or renamed tests fail the check. The current README lists
  every qualified name. This gate does not run native Attic/S3 upload suites.
  The two added legacy Attic regressions are
  `legacy_attic_plaintext_and_historical_ciphertext_retain_on_test_and_save` and
  `legacy_attic_null_and_empty_token_refuse_before_probe_without_mutation` in
  `handlers::api::caches::retained_probe_tests`. They insert historical columns
  directly, check independent `enc:v1` compatibility, exact Test/Cancel snapshots,
  raw credential retention on unrelated Save, and pre-probe NULL/empty refusal.
  Synthetic Rust tokens do not prove native Bearer authentication. The separate
   [historical native browser fixture](https://gitlab.com/crystal-forge/crystal-forge/-/blob/5fa1f4fa/checks/web-ui/README.md#existing-legacy-attic-rows)
  requires real private Attic metadata reads for plaintext/historical rows and
  zero provider requests for the NULL row. Neither catalog membership nor this
  scope description establishes a pass for the current revision.
  The added ignored
  `handlers::api::caches::retained_probe_tests::attic_named_cache_retained_replacement_results_and_policy_preserve_raw_state`
  covers retained/replacement probe results, safe policy refusal, lock release,
  and exact raw-state preservation. Three added non-ignored tests in that module
  are `attic_canonical_api_and_model_read_roots_share_named_cache`,
  `attic_native_metadata_and_status_matrix_never_claims_root_or_write_success`,
  and `attic_target_policy_and_credential_queries_fail_before_network`.
  The fourth is
  `handlers::api::builders::tests::attic_requisite_env_uses_shared_server_base`.
  They cover canonical endpoint/read-root agreement, typed native metadata and
  status classification, pre-network policy, and the requisite server-base export.
  The native fixture separately requires server-root 404 and cache-config 200,
  real CLI setup publication before Test, one named-cache GET per successful
  Test, and zero Test uploads. Corrected runtime proof remains pending owner
  verification. See the
  [Attic endpoint contract](../caches/niks3-cache.md#attic-url-interpretation-and-named-cache-test)
  for normalization, authentication, and compatibility limits.
  The input-owner lifetime regression runs through the same exact-test guard:
  `handlers::api::builders::niks3_input_owner_tests::niks3_input_owner_acknowledges_reap_and_cleanup_after_detach`.
  FIFO handshakes check success, exit failure, timeout, caller detachment, spawn
  failure, and invalid preparation, with completion after reap and credential
  drop. The companion
  `handlers::api::builders::niks3_input_owner_tests::fifo_deadlines_cover_missing_readiness_and_absent_release_reader`
  checks bounded, nonblocking coordination when readiness or a release reader
  is absent. Both tests are selected by the current runner; the coordination
  regression must fail at its deadline rather than hang Cargo.
  Two ignored stored-discovery tests and four non-ignored discovery/certificate
  tests cover transport projection, strict PEM, authorization/CSRF, retained and
  replacement snapshots, mode changes, CA clearing, and non-mutation. The
   current exact-name lists also select Basic-read and all nine storage-metrics
   tests. Basic coverage includes storage/redaction, authority binding, signed
   delivery capability, and independent probe planes. Metrics tests preserve
   native basis, unavailable/null states, authorization, bounds, and non-mutation.
   Selection is blocking-gate coverage, not a claim that the current run passed.
   Completion members now include
   `cache_probe_failure_codes_and_wire_contract_are_static`,
   `cache_probe_exact_root_refresh_attic_and_private_capture_bounds`,
   `niks3_basic_probe_applies_guard_to_path_info_only`,
   `niks3_completion_probe_uses_independent_read_plane`,
   `niks3_probe_failure_never_polls_success_transition`,
   `niks3_probe_timeout_and_cancellation_reap_before_cleanup`, and
   `niks3_pinned_nix_accepts_remote_metadata_and_refresh_flags` in
   `handlers::api::builders::tests`. They prove bounded exact-root metadata reads,
   no copy execution, protected independent credentials, static hints, and cleanup.
   The ignored
   `queries::builders::tests::niks3_pending_publication_guards_rollback_and_later_promotion`
   proves guarded atomic build success,
   exact queue identity/output, later idempotent promotion, rollback, and retained
   historical publication.
   These handler/process tests do not replace the separate native fresh-store
   signature and physical-artifact guards. Builder client tests separately cover
   typed pending/confirmed receipts, retry allowlists, fresh signatures, five-request
   exhaustion, deadlines, and acknowledged-pending retention; they are not members
   of this server-only gate.
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

The runner emits timestamped `START`/`END` phase boundaries, actual exit status,
and elapsed seconds. Eleven critical integration targets each have separate
named compile and run boundaries; the library has a compile boundary, and each
selected TASK-470 test reports its qualified name and mode through the unchanged
exact-test guard. Progress adds no heartbeat, command-argument dump, environment
dump, or `--nocapture`. Earlier aggregate compilation output did not identify a
hung target or the cause of a remote failure; these boundaries aid future diagnosis.

For the checks `server-regressions`, `niks3-cache`, and
`builder-evaluator-packaging`, current CI invokes
`bash packages/ci/public-cache-build.sh` before the normal build attribute.
The `integration-access` job, the full `web-ui-check` job, and the
`web-ui-baseline-candidates` job use the same wrapper. The full browser job and
the candidate job are manual and optional (`allow_failure: true`).
Other matrix entries keep their existing commands. This is command-local policy, not removal of organization
caches from global configuration or a change to the application's Attic access.
The wrapper validates public-only substituters, empty extra substituters, and
rejection of flake-supplied cache settings. It bounds connection attempts to
10 seconds, stalled downloads to 30 seconds, and download attempts to two, with
fallback source builds and signature checks retained. Builder settings remain
inherited; these limits do not bound total compilation or test duration. It
prints the actual Nix version, a boolean inherited-cache indicator, and known
effective settings without exposing unknown runner values. Local configuration
evidence does not establish the remote runner's settings or a CI pass. No manual
baseline action is implied by this catalog entry.

### `builder-evaluator-packaging`

Current successors: [module assertions](../../../modules/nixos/crystal-forge/tests/packaging.nix) and [installed runtime probes](../../../modules/nixos/crystal-forge/tests/packaging.sh). The root and nested module-validation derivations described below are removed.

- **What it verifies.** Sandbox probes check component/public wrappers and service
  PATH for the exact evaluator Nix and evaluator-bound Niks3 1.6.0. The dependent
  module proof checks enabled/disabled proxy assertions, typed generated TOML,
  and the unchanged ExecStart wrapper's selected configuration path and overrides.
- **Run it.** `nix build .#checks.x86_64-linux.builder-evaluator-packaging -L`.
  The smaller proof is `.#checks.x86_64-linux.builder-evaluator-packaging.moduleValidation`.
- **Scope limits.** No VM, host service, browser, or HTTP authorization runs.
  Recording-binary probes establish the startup contract, not live credential
  delivery. Runtime peer/header verification remains required.
- **CI.** Blocking `flake-check: [builder-evaluator-packaging]` matrix job on
  merge requests and `main`, using the `nix` runner tag in `.gitlab-ci.yml`.

### `niks3-cache`

Historical README: [checks/niks3-cache/README.md](https://gitlab.com/crystal-forge/crystal-forge/-/blob/5fa1f4fa/checks/niks3-cache/README.md).

- **What it verifies.** Four isolated VMs exercise five token/mTLS write and
  public/private read variants with real remote builds, signed destination-bound
   completion, server refreshed exact-output metadata verification, agent pulls, environment
  isolation, local CVE materialization, and secret/cleanup audits. Real HTTPS
  proxy claims cover Attic/S3/Niks3 credential gates and public Http/Nix dispatch.
- **Run it.** `nix build .#checks.x86_64-linux.niks3-cache -L`. The full gate has
  `globalTimeout = 1800` seconds and requires KVM. Disposable VM databases do not
  use the host development database. `.#checks.x86_64-linux.niks3-cache.fixture`
  provides smaller CLI/infrastructure diagnosis; it does not replace the full gate.
- **Scope limits.** Seeded evaluation identities do not test flake evaluation.
  Agent pulls do not prove a full generation switch/reboot; CVE materialization
  does not prove successful vulnerability analysis. Legacy claims do not prove
   native Attic/S3 uploads. Completion does not copy or recursively validate the
   closure. Native fixture fresh-store reads, agent pulls, and CVE materialization
   retain signature/NAR-integrity checks independently. See the README for
   certificate and log-audit limits and the
   [completion API](../api/builder-job-lifecycle-api.md#post-apiv1buildersidjobsjob_idcomplete)
   for guarded HTTP 200/202 receipts and bounded callback recovery.
- **CI.** Optional manual `legacy-flake-check: [niks3-cache]` job
  (`allow_failure: true`) on merge requests and `main`, using the `nix` runner
  tag in `.gitlab-ci.yml`; its runner must supply KVM. It was a blocking
  `flake-check` matrix job before the TASK-490.24 early transition. A manual
  job establishes no pass for a particular commit, and its obligations are not
  covered by `integration-access`.

## Web UI checks

The expanded native Niks3 graph adds Basic reads and independent API/storage CA
roots. Basic requires actual runtime `cf-netrc-authority` support and exact-origin,
no-redirect protection. The two-root write case trusts API and presigned storage
servers independently of the client certificate issuer. A separate native 1.8
database/bucket tests stats; production packaging remains Niks3 1.6. Discovery
and Test cannot prove write authorization from TLS acceptance. Earlier mTLS-only
results do not verify this graph. The current native guard run failed its
NAR-host expected-error assertion; final expanded native and browser verification
remains pending owner evidence. No current-head CI pass is established here.

### `web-ui`

Historical README: [checks/web-ui/README.md](https://gitlab.com/crystal-forge/crystal-forge/-/blob/5fa1f4fa/checks/web-ui/README.md). The
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
- **CI.** The check runs through the separate `web-ui-check` job. That job is
  an optional manual job with `allow_failure: true`; it does not run
  automatically. The automatic `web-ui-screenshots-mr-comment` reporter was
  removed. Its obligations are not covered by `integration-access`.

`checks/web-ui/default.nix` sets `globalTimeout = 3000` and
`playwrightResultTimeout ? 2700`. `.gitlab-ci.yml` runs the Web UI check as a
separate manual `web-ui-check` job with `allow_failure: true`. The automatic
`flake-check` matrix is `run-ui-dev-db-check`, `server-regressions`,
`nixos-discovery-reliability`, `builder-evaluator-packaging`,
`web-ui-test-runner`, and `okf-knowledge`. The manual `legacy-flake-check`
matrix is `integration`, `oidc-auth`, `niks3-cache`, `niks3-write-probe`,
`niks3-ui`, `attic-metrics`, and `attic-publication`.

### `web-ui/baselines`

Historical README: [checks/web-ui/baselines/README.md](https://gitlab.com/crystal-forge/crystal-forge/-/blob/5fa1f4fa/checks/web-ui/baselines/README.md).

This directory holds reviewed visual baselines for `strict` manifest steps. It
is not a check. Generate captures through the `web-ui` check and approve them
with `checks/web-ui/approve-baselines.sh`, then commit the PNG files. The
README forbids adding failed-step diagnostics, diff images, reports, or
export-test screenshots to this directory. The approval procedure is in the
[Web UI check runbook](web-ui-check.md).

### `web-ui-reconciliation`

Historical README: [checks/web-ui-reconciliation/README.md](https://gitlab.com/crystal-forge/crystal-forge/-/blob/5fa1f4fa/checks/web-ui-reconciliation/README.md).

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

Current successor: [direct host runner selftest](../../../packages/devScripts/web-ui/tests/web-ui-test-runner-test.sh).

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

README: [packages/ui-screenshots/README.md](../../../packages/ui-screenshots/README.md).

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

Current successor: [ordinary OSCAL fixture package](../../../packages/oscal-fixture/default.nix) with its [schema-chain validator](../../../packages/oscal-fixture/validate.py).

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

Current successor: [installed writer schema validation](../../../packages/default/crates/cf-server/tests/xccdf-schema.sh). Hand-authored XML below is historical static-example validation, not current production-test evidence or proof of all export variants.

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

Current successor: [eleven pure module assertion groups](../../../modules/nixos/crystal-forge/tests/stig.nix), evaluated directly without a derivation.

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

Current successor: [ordinary metadata package assertions and postCheck](../../../packages/nixos-options-metadata/default.nix).

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

Current successors: [DB usability selftest](../../../packages/devScripts/db-usability-check-test.sh) and [DB start selftest](../../../packages/devScripts/db-only-start-test.sh), invoked directly under `nix develop`.

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

- `nix run .#cf-test-suite.scenarioRunner -- -h` prints help.
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

The package is `packages/cf-test-suite/`. Its `scenarioRunner` member builds
`cf-scenarios`; the development shell alias `run-db-test` runs
`.#cf-test-suite.runTests`.

## Checks without a README

At the migration base commit, these directories under `checks/` have no
README: `builder-cve-contract`,
`builder-evaluator-packaging`, `config-inspector`, `config-observer`,
`evaluator-snapshot-isolation`, `okf-knowledge`, `test-keys`, and
`verified-source-evaluator-parity`. TASK-470 subsequently added the
`builder-evaluator-packaging` README and catalog section above. Read each
remaining check's `default.nix` for its scope.

## Related concepts

- [Web UI check runbook](web-ui-check.md)
- [Crystal Forge testing plan](test-plan.md)
- [Fixture seeding developer guide](fixture-seeding.md)
- [Golden fixtures contract](design-golden-fixtures.md)
- [Offline flake input prefetching for NixOS VM tests](offline-flake-prefetch.md)

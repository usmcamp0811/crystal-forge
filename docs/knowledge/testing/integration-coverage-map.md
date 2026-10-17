---
type: Testing Guide
title: Crystal Forge integration coverage and retirement map
description: "Maps every accepted integration obligation to its single owner check, scenario, independent expected result, evidence, and status; documents the shared harness interface; and gives a retirement disposition for every legacy check directory and CI job."
tags:
  - crystal-forge
  - testing
  - integration
  - coverage
implementation_status: partial
generated:
  by: anthropic/claude-opus-5-5
  at: 2026-10-17T11:40:00-05:00
---

# Crystal Forge integration coverage and retirement map

This map is maintained under the [integration test maintenance contract](integration-test-contract.md). It is the compact catalog that contract Section 3 requires. It records **status**, not intent: a row is `covered` only when its scenario executes and passes at a recorded revision.

Application baseline: `dev` at `33de73b013b735304dfac9c69745a074a3e76ebd`. Overhaul branch: `TASK-490.1-integration-foundation`. Parent task: TASK-490.

Status values:

| Status | Meaning |
| --- | --- |
| `covered` | Scenario executed and passed at the recorded revision. |
| `failing: defect` | Scenario executes; a correct assertion fails because of a recorded product defect. Not covered. |
| `planned` | A child task owns the scenario; it is not implemented. |
| `unverified` | Scenario exists, but has not executed to establish its outcome. |
| `blocked: design gap` | Accepted domain behavior has no approved UI state. The dependent UI case waits. |
| `blocked: decision` | An owner decision is needed before an expectation can be authored. |
| `gap: unimplemented` | Accepted requirement that the application does not implement. Recorded, never shown as coverage. |

## Authority decision D1 (pending owner confirmation)

Most current behavior is described by `docs/knowledge/` concepts with `implementation_status: implemented`. Those documents describe as-built behavior; the contract says as-built behavior does not approve itself. Approved design authority exists for Systems/SC1 (`systems-view-design-v0.1.md` Section 22 owner decisions) and the owner-approved CVE/POA&M continuity specification. The compliance, CVEs, cross-view, and XCCDF design documents are review drafts.

No provisional authority is granted by D1. Implemented knowledge concepts are as-built evidence, not accepted requirements. Review-draft design documents also do not approve domain behavior. A scenario must cite an accepted requirement or explicit owner decision for its expectations. Uncertain cases remain `blocked: decision` pending the owner's decision; a `planned` row below identifies intended work, not approval of its expected result. Where an approved source exists, that source controls the expectation even if current code differs.

## Harness interface

Shared helpers and registration points:

| Path | Responsibility |
| --- | --- |
| `lib/integration-test/default.nix` | Snowfall export: `lib.crystal-forge.makeIntegrationTestHarness {pkgs, inputs}`; pass both from the native check arguments. |
| `lib/integration-test/harness.nix` | Harness implementation: production `packages`, `roleNames`, and `mkScenario`. |
| `lib/integration-test/roles.nix` | Role modules. `common role`; `controller { browser ? false }`. Other roles: TASK-490.7. |
| `lib/integration-test/browser.nix` | Render profile `cf-it-render-v1`, `cf-browser` wrapper, browser NixOS module. |
| `lib/integration-test/browser/cf-browser.cjs` | Browser spec runner and `ctx` API. |
| `lib/integration-test/facts.nix` | `byId`, `localUserFromDesign`, `designReference`, `designSite`. |
| `lib/integration-test/driver.py` | Python helpers available in every scenario body. |
| `checks/integration-access/default.nix` | Coordinator-owned explicit `scenarios` attribute set; normal check builds all registered gates with `pkgs.linkFarm`. |
| `checks/integration-access/scenarios/<name>.{nix,py,cjs}` | One Access scenario. Child tasks write only their scenarios, fixtures, and assets beneath the owning check directory. |

Registration is explicit and coordinator-owned; there is no custom discovery or completeness flag. The normal `checks.x86_64-linux.integration-access` check builds every gate in its `scenarios` attribute set. Meaningful WIP checks may be discoverable; only Access currently is. Missing obligations remain in this map. Future Delivery, Assurance, and Upgrade scenarios, fixtures, and assets all belong beneath `checks/integration-{delivery,assurance,upgrade}/`. Do not create placeholder `default.nix` files. Legacy checks and CI selection remain unchanged until all required owner scenarios pass.

Scenario file contract (`<name>.nix`):

```nix
{ harness, owner, name }:
harness.mkScenario {
  inherit owner name;
  contracts = [ "access.lifecycle" ];          # coverage-map contract IDs
  nodes.controller = harness.roles.controller { browser = true; };
  facts = { ... };                              # authored, JSON-serializable
  script = ./<name>.py;                         # Python body, runs in driver
  browserSpecs.<case> = ./<name>.cjs;           # optional browser specs
  timeout = 900;                                # global VM timeout, seconds
}
```

Python helpers (`driver.py`), available in the scenario body:

| Helper | Contract |
| --- | --- |
| `FACTS` | Authored facts (`/etc/cf-it/facts.json` in every VM). |
| `case(name)` | Context manager for one named mandatory case. The first failure stops the scenario. |
| `expect(cond, msg)`, `expect_equal(actual, expected, what)` | Raise `ScenarioFailure`. |
| `receipt(key, value)` / `receipts()` | Write-once runtime identities (generated IDs, store paths). |
| `wait_for(predicate, what, timeout, interval=1.0)` | Bounded polling with a named condition. |
| `http(node, method, path, body=None, session=None, csrf=False, base="http://localhost:3000")` | curl from `node`; returns `(status, json_or_text)`; named cookie jars; `csrf=True` sends `x-csrf-token` from the jar. |
| `sql_json(node, query, database="crystal_forge")` | Read query as postgres; returns a list of dicts. Authoritative reads only. |
| `run_browser(node, spec, viewport="desktop", base_url=...)` | Runs `/etc/cf-it/specs/<spec>.cjs`; merges emitted receipts; copies artifacts. |
| `assert_store_absent(node, path)` | Fails if the path is valid or present as a file. |
| `unit_stopped(node, unit)`, `unit_killed(node, unit, signal)`, `network_blocked(node, host, port=None)` | Named, recorded fault injection. |

Browser `ctx` (`cf-browser.cjs`): `page`, `context`, `browser`, `expect` (bounded to `defaultTimeoutMs` = 15000), `facts`, `receipts`, `emit(key, value)`, `url(path)`, `step(name, fn)`, `screenshot(name)`, `download(name, trigger)`, `whoami()`, `visual.compare(name, {view, theme, referenceSelector, actualSelector, maxDifferentRatio})`. `visual.compare` renders the approved design reference from `designSite` in the same browser and profile, writes `<name>.reference.png`, `<name>.actual.png`, `<name>.diff.png`, and fails above the explicitly stated ratio. No baseline is stored, so no run can update a baseline. `visual.compare` has not yet been exercised by an executed scenario; the first visual case validates it.

Render profile `cf-it-render-v1`: Playwright 1.59.1 and its Chromium from the locked `nixpkgs`; fonts DejaVu Sans/Serif and JetBrains Mono only; locale `en-US`; timezone `UTC`; color scheme dark; reduced motion; device scale 1; viewports `desktop` 1440×900 and `narrow` 375×812.

Evidence: `nix build .#checks.x86_64-linux.integration-access.scenarios.lifecycle-local-bootstrap -L` builds a gate. The gate's `evidence` link is the run output: `result.json` (status, cases, durations, faults, receipts, error, traceback), `meta.json` (source revision, contracts, render profile, design reference and fixture hash), `browser/<spec>/` (`result.json`, `console.log`, screenshots, `trace.zip` and `failure.png` on failure, downloads, visual images), `journals/<node>/` (`crystal-forge*` and PostgreSQL journal, warnings, failed units). On scenario failure, run `nix build .#checks.x86_64-linux.integration-access.scenarios.lifecycle-local-bootstrap.run -L` to obtain the evidence path; that derivation is evidence only and is never a gate. A package-build failure before VM execution produces no scenario execution evidence.

Isolation and shared store: each scenario is its own NixOS test with fresh VMs and databases. VMs see host store files through a read-only mount but register only their own closure. A build, publication, or copy proof MUST use an output first produced inside the test (per-run nonce) and MUST call `assert_store_absent` at the destination first.

Host resources (`sledge`, inspected 2026-10-17): 32 cores, 123 GiB RAM with about 45 GiB available. The controller is configured for 4 GiB and 4 cores. No VM run has completed, so runtime, peak usage, and safe parallel capacity are unmeasured. Run one VM test at a time until measurements establish capacity.

## Obligation map

Scenario paths are relative to `checks/`. "Evidence" names the observation required in addition to `result.json`. Expected results still require the authority review described in D1.

### Access (`integration-access`)

| Contract ID | Scenario | Independent expected result | Evidence | Status |
| --- | --- | --- | --- | --- |
| `access.lifecycle.local-bootstrap` | `integration-access/scenarios/lifecycle-local-bootstrap` | Module `auth_mode = "local"` yields local mode; migrations applied = forward migration files in source; first registrant persists with only `admin`; one session after sign-in survives reload; wrong password returns 401 with no session row or cookie. | DB rows, browser trace on failure, screenshot after reload | `unverified`; see [execution record](#foundation-execution-record) |
| `access.lifecycle.sessions` | TASK-490.2 | Sign-out invalidates the session row and cookies; missing or mismatched `x-csrf-token` returns 403 with no write; revoked session is rejected on next request. | DB `user_sessions`, protected-row before/after | `planned` |
| `access.lifecycle.authorization` | TASK-490.2 | Operator and Viewer accounts see the approved unavailable state and receive 403 for admin mutations; denied requests change no protected row. | DB before/after, browser state | `planned` |
| `access.lifecycle.registration-closed` | TASK-490.2 | With `allow_registration = false` and an existing user, a second registration does not create a user. | DB `users` count | `planned` (code reading suggests a defect; not yet executed) |
| `access.lifecycle.oidc` | TASK-490.6 | Authorization-code sign-in through a local OIDC provider creates a CF session; group mapping yields the mapped role and environment scope. | Provider and CF DB rows, browser trace | `planned` (needs `services` role) |
| `access.configuration.admin` | TASK-490.3 | Admin users, classification, and retry-policy forms persist and survive reload; cancel discards; a failed save keeps the draft. | DB rows, reload read | `planned` |
| `access.configuration.resources` | TASK-490.3 | Environment create/edit/delete persists through reload; validation errors keep the draft. | DB rows, reload read | `planned` |
| `access.shell` | TASK-490.4 | Navigation reaches every accepted route; unknown routes show NotFound; keyboard focus and sidebar collapse follow the approved Shell reference; unauthenticated routes redirect to `/login`. | Screenshots, visual comparison where an approved state exists | `planned` |
| `access.configuration.preferences` | TASK-490.4 | Profile and notification preferences persist per user and survive reload; one user's change does not affect another. | DB `user_preferences`, reload read | `planned` |
| `access.configuration.notifications` | TASK-490.5 | A real domain event produces an in-app notification; read/dismiss persist; email delivery reaches the local receiver only when enabled. | Receiver log, DB rows | `planned` (needs event source and `services` role) |

### Delivery (`integration-delivery`)

| Contract ID | Scenario | Independent expected result | Evidence | Status |
| --- | --- | --- | --- | --- |
| `delivery.evaluation.ingestion` | TASK-490.8 | Push and webhook events for authored commits create exactly one commit row each; duplicate events are deduplicated; force-push rewrite is detected. | Git log on services, DB rows | `planned` |
| `delivery.evaluation.identity` | TASK-490.8 | Evaluation of two distinguishable commits records the exact commit and the `drvPath` that `nix eval` independently computes; Evaluations view navigation reaches the exact record. | Independent `nix eval` output, browser trace | `planned` |
| `delivery.evaluation.config-inspector` | TASK-490.9 | Config Inspector lists the authored option values and definition provenance for the exact revision; a secret-path value is redacted. | Independent option facts, browser state | `planned` |
| `delivery.build.strategies` | TASK-490.10 | `ServerDerivation` and `SourceReEvaluateVerified` each build a fresh output on a real builder; an unauthorized builder session is rejected with no job state change. | Builder store, DB job rows | `planned` |
| `delivery.build.scheduling` | TASK-490.11 | Cancel, retry, reassignment, and stale-session fencing produce the specified job states; a fenced builder cannot complete a job. | DB job history, builder logs | `planned` |
| `delivery.publication.niks3` | TASK-490.12 | A fresh output is published to Niks3 with token and mTLS write auth; a cold agent store retrieves it with signature verification. | `assert_store_absent`, cache object, agent store | `planned` |
| `delivery.publication.variants` | TASK-490.13 | Attic, S3, and HTTP/Nix each prove their distinct publication or read authentication boundary once; the common workflow is not repeated. | Per-adapter observer logs | `planned` |
| `delivery.activation.modes` | TASK-490.14 | `manual`, `auto_latest`, and `pinned` deploy exactly as specified; `BootOnly` changes the booted but not the running generation; a second agent is not affected. | Target `/run/current-system`, boot entry, DB | `planned` |
| `delivery.activation.recovery` | TASK-490.15 | Rollback restores the previous generation on the target; drift is reported; server, builder, and agent restarts recover in-flight state at the current version. | Target state, DB, journals | `planned` |

### Assurance (`integration-assurance`)

| Contract ID | Scenario | Independent expected result | Evidence | Status |
| --- | --- | --- | --- | --- |
| `assurance.scanning.cve` | TASK-490.16 | A real scanner run against the intended closure with deterministic vulnerability data yields the authored CVE set; Current, built, historical, missing, failed, and clean evidence stay distinct in API and UI. | Scanner input data, DB rows, browser state | `planned` |
| `assurance.scanning.hardening` | TASK-490.17 | Hardening scan of a real target reports the authored service findings; justification persists. | Target unit facts, DB | `planned` |
| `assurance.policy.enforcement` | TASK-490.18 | Policy versions and assignments resolve to the expected effective policies; `Enforce` blocks and `ReportOnly` records without blocking an admission decision. | DB decisions, deployment outcome | `planned` |
| `assurance.policy.stig-import` | TASK-490.19 | Importing the DISA Anduril NixOS STIG XCCDF yields the authored rule count and identities; preview does not persist; malformed input is rejected atomically. | DB rows before/after, browser state | `planned` |
| `assurance.remediation.triage` | TASK-490.20 | Triage and finding waivers persist with actor and scope; waiver does not mark remediation. | DB rows | `planned` |
| `assurance.remediation.poam` | TASK-490.21 | POA&M creation from persisted evidence, milestones, verification, closure, and reopen preserve finding identity and evidence continuity. | DB rows, browser state | `planned` |
| `assurance.interchange` | TASK-490.22 | Downloaded register (CSV, XLSX, OSCAL JSON/XML), POA&M, acceptance, CVE CSV, XCCDF, and policy-interchange exports parse independently and contain the expected records and scope. | Downloaded files, schema validation | `planned` |

### Upgrade (`integration-upgrade`)

| Contract ID | Scenario | Independent expected result | Evidence | Status |
| --- | --- | --- | --- | --- |
| `upgrade.continuity` | TASK-490.23 | A supported older package creates users, sessions, history, and in-flight work through its own schema; the current package migrates and keeps identities and credentials, recovers in-flight state, and completes one new operation. | Old/new DB rows, browser sign-in | `blocked: decision` |

Decision D2 (Upgrade): no document defines the supported upgrade source or the agent/builder compatibility window. Tags exist (`v0.3.1` 2026-05-30, `v0.3.0`, `v0.2.x`), and all crates report `0.3.0`; a tag alone is not a support promise. The owner must name (a) the oldest supported server release for in-place upgrade, (b) whether agents and builders from that release must keep working against the current server, and (c) the pinned source for that release. A same-version restart, a synthetic old schema, or current-version seeded data is not a substitute. The migration-prefix rehearsals in `server-regressions` remain non-integration schema tests.

### UI inventory and owning workflow

Routes from `packages/web-ui/src/routes.rs`. "Approved reference" names the design component; `none` means a visual case is `blocked: design gap` and only semantic assertions apply.

| Route / surface | Owning workflow | Approved reference |
| --- | --- | --- |
| `/login`, `/register`, `/setup` | `access.lifecycle` | none |
| Shell, sidebar, user menu, `/:..route` NotFound, `/style-guide` | `access.shell` | `Shell.jsx` |
| `/profile` | `access.configuration.preferences` | `ProfileView.jsx` |
| `/admin` (users, roles, oidc, jobs, audit, server tabs) | `access.configuration.admin` | `AdminView.jsx` |
| `/environments` and its modals | `access.configuration.resources` | `EnvironmentsView.jsx` |
| `/` dashboard | Mechanics `access.shell`; counts by domain owner | `DashboardView.jsx`, `DashboardWidgetsOps.jsx` |
| `/flakes`, history-rewrite dialog | `delivery.evaluation.ingestion` | `FlakesView.jsx`, `FlakeExplorer.jsx` |
| `/evaluations`, `/evaluations/:commit_id`, EvalDrawer | `delivery.evaluation.identity` | `EvalsView.jsx`, `EvalDrawer.jsx` |
| System Config tab, flake Config Inspector | `delivery.evaluation.config-inspector` | `ConfigExplorer.jsx` |
| `/builders`, `/builds` | `delivery.build.*` | `BuildersView.jsx`, `BuildsView.jsx` |
| `/caches` and credential modals | `delivery.publication.*` (form persistence: Access) | `CachesView.jsx`, `Niks3Cache.jsx` |
| `/systems`, `/systems/:id` Overview, Deploy, History, Logs | `delivery.activation.*` | `Systems.jsx`, `SystemDetail.jsx`, `DeployGate.jsx` |
| System CVEs tab, `/cves`, `/scanning` | `assurance.scanning.cve` | `CvesView.jsx`, `ScanningView.jsx`, SC1 design |
| System Hardening tab | `assurance.scanning.hardening` | `HardeningTab.jsx` |
| `/deployment-policies`, policy editor | `assurance.policy.*` | `PoliciesView.jsx`, `PolicyEditor.jsx` |
| `/compliance` bundles, evidence drawer, Import STIG | `assurance.policy.*` | `ComplianceView.jsx`, `BundleEditor.jsx`, `ImportStigModal.jsx` |
| `/poams` and triage dialogs | `assurance.remediation.*` | `PoamsView.jsx`, `PoamViews.jsx`, `CveBatchTriage.jsx` |
| Export modal and register downloads | `assurance.interchange` | `AtoPackage.jsx` |

Shared mechanics (drawer focus/close, confirm dialogs) are tested once in the first owning workflow; each distinct caller's identity, permission, and return destination is asserted in its own workflow.

### Supported variants at the baseline

Verified in code at the baseline: cache types `S3`, `Attic`, `Niks3`, `Http`, `Nix` (`cf-protocol/src/cache.rs`); builder strategies `ServerDerivation`, `SourceReEvaluateVerified`, with source delivery `ServerBundledArchive` only (`cf-protocol/src/builder.rs`); system deployment policies `manual`, `auto_latest`, `pinned` and activation `ImmediatePersist`, `BootOnly`; enforcement `Enforce`, `ReportOnly`; auth modes `local`, `oidc` (module); roles `Admin`, `Operator`, `Viewer`; notification channels `InApp`, `Email`, `Both`; imports XCCDF 1.2 XML/ZIP and policy interchange; server exports register `csv`/`xlsx`/`oscal-json`/`oscal-xml`, POA&M and acceptances `csv`/`xlsx`, CVEs `csv`, XCCDF, policy interchange; client-only CF-JSON, CSV, SARIF, OSCAL, print HTML. No CKL importer exists. A feature named only in a design draft is not in scope.

## Suspected product defects and design gaps

| ID | Kind | Summary | Contract | Task |
| --- | --- | --- | --- | --- |
| P1 | Suspected product defect; not reproduced | Code inspection suggests that emitting `AUTH_MODE` only for `oidc` and omitting `auth_mode` from generated TOML could prevent local sign-in under the default module configuration. No scenario VM has executed to verify this suspicion. | `access.lifecycle.local-bootstrap` | See [execution record](#foundation-execution-record) |
| G1 | Design gap | No approved design component for `/login`, `/register`, or `/setup`. | `access.lifecycle` visual cases | None; semantic assertions only |

## Legacy retirement map

Disposition values: `replace` (retire after the required owner scenarios pass), `keep` (non-integration check; keeps its own attribute), `duplicate`, `obsolete`. Legacy checks and CI selection remain unchanged until all required owner scenarios pass.

| Legacy check or job | Kind | Disposition | Replacement owner / scenario or reason |
| --- | --- | --- | --- |
| `checks/web-ui` (CI `web-ui-check`, allow_failure) | Integration, browser | `replace` | Access, Delivery, Assurance browser scenarios. 105 of 181 steps use route mocks; mocked steps are not carried forward as integration proof. Authentic real-API steps (03/05 registration and login, 13h force-push, 20aa/20ab/20ad/20af, 21–25, 29g–29m, 30b/30d/30e) map to `access.*`, `delivery.evaluation.ingestion`, `delivery.publication.*`, `assurance.*`. |
| `checks/niks3-ui` (CI matrix) | Integration | `duplicate` | Re-runs web-ui step 25 and export phases; replaced by `delivery.publication.niks3`. |
| `checks/web-ui-reconciliation` | Browser, fully mocked | `obsolete` | Every API response mocked; replaced by `assurance.policy.stig-import`. |
| `checks/integration` (CI matrix) | Integration | `replace` | `delivery.evaluation.ingestion` (polling, webhook, branches), `delivery.activation.*` (agent accept, desired target). Grafana and SQL-view assertions are not product integration obligations (Grafana is optional/legacy). |
| `checks/oidc-auth` (CI matrix) | Integration, shallow | `replace` | `access.lifecycle.oidc` (adds callback, session, mapping). |
| `checks/niks3-cache` (CI matrix) | Integration | `replace` | `delivery.publication.niks3`, `delivery.build.strategies`. Seeded queue rows are replaced by real evaluation. |
| `checks/niks3-write-probe` (CI matrix) | Integration | `replace` | `delivery.publication.niks3` (connection Test boundary). |
| `checks/attic-metrics` (CI matrix) | Integration | `replace` | `delivery.publication.variants` (Attic). |
| `checks/attic-publication` (CI matrix) | Component | `replace` | `delivery.publication.variants` (Attic publication scope). |
| `checks/nixos-discovery-reliability` (CI matrix) | Nix contract, no VM | `keep` | Non-integration evaluator contract. |
| `checks/server-regressions` (CI matrix) | DB-backed Rust | `keep` | Non-integration. Migration-prefix rehearsals do not satisfy `upgrade.continuity`. |
| `checks/builder-evaluator-packaging` (CI matrix) | Packaging | `keep` | Non-integration. |
| `checks/web-ui-test-runner` (CI matrix) | Dev tooling | `keep` until `web-ui` retires, then `obsolete` | Tests the legacy host runner. |
| `checks/run-ui-dev-db-check` (CI matrix) | Dev tooling | `keep` | Non-integration. |
| `checks/okf-knowledge` (CI matrix) | Documentation | `keep` | Non-integration. |
| `checks/ui-screenshots` | Mocked screenshots | `keep` (non-integration) | Design iteration aid; no semantic assertions. |
| `checks/config-inspector`, `config-observer`, `evaluator-snapshot-isolation`, `verified-source-evaluator-parity`, `nixos-options-metadata`, `niks3-ca-bundle` | Nix/Rust contracts | `keep` | Non-integration. |
| `checks/builder-cve-contract`, `oscal-export`, `xccdf-schema`, `stig` | Contracts/unit | `keep` | Non-integration. |
| `checks/test-keys` | Not a check | `obsolete` | Keys are embedded elsewhere; remove at cutover if unused. |
| CI `cve-processing-test`, `dashboard-visibility-tests`, `state-machine-tests` | DB-backed Rust | `keep` | Non-integration; the fake scanner runner does not satisfy `assurance.scanning.cve`. |

At cutover (TASK-490.24), after all required owner scenarios pass, the integration CI entry point runs the four owners and checks the four-name allowlist; replaced legacy registrations, their CI selections, and the legacy `web-ui` fixtures (`checks/web-ui/tests/fixtures/*.xml` move beneath `checks/integration-assurance/`) are removed in the same change.

## Foundation execution record

Bootstrap execution is **unverified**. Two package-build attempts failed in different existing Rust tests before any scenario VM executed. Neither attempt establishes a reproduced product defect, a scenario failure, or a passing integration result. No VM execution evidence is available yet. TASK-491 records the package-build failures and evidence logs.

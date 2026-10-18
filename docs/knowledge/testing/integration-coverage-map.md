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
| `lib/integration-test/default.nix` | Snowfall export: call `lib.crystal-forge.makeIntegrationTestHarness { inherit pkgs inputs; }` from native check arguments. |
| `lib/integration-test/harness.nix` | Harness implementation: production `packages`, `roleNames`, and `mkScenario`. |
| `lib/integration-test/roles.nix` | Role modules. `common role`; `controller { browser ? false }`. Other roles: TASK-490.7. |
| `lib/integration-test/browser.nix` | Render profile `cf-it-render-v1`, `cf-browser` wrapper, browser NixOS module. |
| `lib/integration-test/browser/cf-browser.cjs` | Browser spec runner and `ctx` API. |
| `lib/integration-test/facts.nix` | `byId`, `localUserFromDesign`, `designReference`, `designSite`. |
| `lib/integration-test/driver.py` | Python helpers available in every scenario body. |
| `checks/integration-access/default.nix` | Coordinator-owned explicit `scenarios` attribute set; normal check builds all registered gates with `pkgs.linkFarm`. |
| `checks/integration-access/scenarios/<name>.{nix,py,cjs}` | One Access scenario. Coordinator owns the scenario `.nix`, Python composition, and explicit registration. Packet tasks write disjoint Python/browser case fragments and fixtures under `scenarios/fragments/` or `fixtures/`; one packet does not imply one test derivation. |

Registration is explicit and coordinator-owned; there is no custom discovery or completeness flag. The normal `checks.x86_64-linux.integration-access` check builds every gate in its `scenarios` attribute set. Meaningful WIP checks may be discoverable; only Access currently is. An implementation packet does not automatically get a NixOS test derivation. Compatible packet cases are fragments composed by the coordinator into a shared scenario using the existing Nix, Python, and browser-script structure. Missing obligations remain in this map. Future Delivery, Assurance, and Upgrade scenarios, fixtures, and assets all belong beneath `checks/integration-{delivery,assurance,upgrade}/`. Do not create placeholder `default.nix` files. Legacy checks and CI selection remain unchanged until all required owner scenarios pass.

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

Isolation and shared store: each separately built scenario is its own NixOS test with fresh VMs and databases. Later cases in one scenario may use identities and rows created by earlier real workflows, carried through receipts. A task dependency never provides live state from another scenario. VMs see host store files through a read-only mount but register only their own closure. A build, publication, or copy proof MUST use an output first produced inside that scenario (per-run nonce) and MUST call `assert_store_absent` at the destination first.

Host resources (`sledge`, inspected 2026-10-17): 32 cores, 123 GiB RAM with about 45 GiB available. The controller is configured for 4 GiB and 4 cores. One VM run failed during server startup; peak usage and safe parallel capacity remain unmeasured. Run one VM test at a time until measurements establish capacity.

## Obligation map

Scenario paths are relative to `checks/`. "Evidence" names the observation required in addition to `result.json`. Expected results still require the accepted authority review described in D1. Each separately built scenario runs in a fresh VM/database; a prerequisite scenario's runtime output is never reused. A task dependency supplies code/immutable fixture preparation only, not another check's live data. Within one scenario, later cases may use identities and records created by earlier real workflows; carry generated identities in the existing receipt mechanism. Every scenario must create its own real starting evidence through setup operations and must not seed the result under test. The coordinator owns composition and registration; packet boundaries alone do not define VM boundaries.

### Planned packet-to-scenario composition and VM boundaries

The current table records intended case composition, not implemented coverage. The coordinator owns each scenario's `.nix` composition and explicit owner registration. Packet-owned case fragments have disjoint paths under the owner's `scenarios/fragments/` directory; they do not each define or register a NixOS test derivation.

| Scenario | Packet cases to compose | Why this VM boundary is retained |
| --- | --- | --- |
| `integration-access/scenarios/lifecycle-local-bootstrap` | TASK-490.1 bootstrap plus local-mode session/CSRF/sign-out, authorization, closed registration, admin/environment forms, shell/navigation, and per-user preferences from TASK-490.2/.3/.4. Each case uses identities and records produced earlier in this same real workflow; generated identities go through receipts. | One local-auth controller configuration and one evolving database support these cases. No separate packet VM is justified. |
| `integration-access/scenarios/configuration-notifications` | TASK-490.5 real event, in-app preference/delivery, and email delivery cases. | Separate from local bootstrap because proving email requires a different machine configuration: notification email enabled with a local receiver, plus a real delivery/build event. Recreate that event in this scenario; TASK-490.11's VM or output is not reusable. |
| `integration-access/scenarios/lifecycle-oidc` | TASK-490.6 authorization-code, callback, role mapping, and sign-out cases. | Separate because `auth_mode = "oidc"` and the local OIDC provider/redirect setup are incompatible with the local-mode bootstrap proof. |
| `integration-delivery/scenarios/delivery-lifecycle` | TASK-490.8/.9/.10/.11/.12/.13/.14/.15: real ingestion/evaluation, build scheduling/transports, publication variants, activation, rollback, and recovery cases. Use generated commit/build/cache/target identities through receipts. | One controller, service fixture, builder, second agent, and target topology supports the ordered workflows. Keep all produced outputs and target state inside this run. No task-specific Delivery VM is justified by the current module/role requirements. |
| `integration-assurance/scenarios/evidence-remediation-lifecycle` | TASK-490.16/.17/.18/.19 scanning, target, policy, and import cases followed by TASK-490.20/.21/.27/.28/.29 triage, waiver, acceptance, POA&M, verification/closure/reopen, then TASK-490.22 exports of records created earlier in the same scenario. | One controller/target/service configuration and a single actual finding/evidence chain can support these ordered cases. Keep one fresh VM/database for the whole scenario; use receipts for finding, acceptance, plan, and target identities. Do not create packet-specific derivations or depend on another scenario's outputs. Reconsider only if a proven incompatible machine configuration or isolation invariant appears, and record that concrete reason here before splitting. |
| `integration-upgrade/scenarios/version-transition` | TASK-490.23 old-version seed workflow, current-version migration, continuity and recovery cases. | Separate because it intentionally changes the running server version and migrates an in-place database; that compatibility/migration state must not contaminate current-version Delivery or Assurance evidence. Old and current phases remain sequential inside its own fresh VM/database. |

The planned scenario derivations are the local Access lifecycle, notification delivery, OIDC lifecycle, Delivery lifecycle, Assurance evidence/remediation lifecycle, and Upgrade version transition. Their boundaries reflect incompatible auth/service configuration, separate owner contracts, or deliberate in-place version/database transition—not one VM per implementation packet. No independent scenario may use another scenario's rows, sessions, receipts, generations, or output.

### Access (`integration-access`)

| Contract ID | Scenario | Independent expected result | Evidence | Status |
| --- | --- | --- | --- | --- |
| `access.lifecycle.local-bootstrap` | `integration-access/scenarios/lifecycle-local-bootstrap` | Module `auth_mode = "local"` yields local mode; migrations applied = forward migration files in source; first registrant persists with only `admin`; one session after sign-in survives reload; wrong password returns 401 with no session row or cookie. | DB rows, browser trace on failure, screenshot after reload, generated-config summary | `unverified` after separate WIP repairs TASK-490.25/.26; combined VM rerun pending |
| `access.lifecycle.sessions` | `integration-access/scenarios/lifecycle-local-bootstrap` (TASK-490.2 fragment) | Sign-out invalidates the session row and cookies; missing or mismatched `x-csrf-token` returns 403 with no write; revoked session is rejected on next request. | DB `user_sessions`, protected-row before/after | `planned` |
| `access.lifecycle.authorization` | `integration-access/scenarios/lifecycle-local-bootstrap` (TASK-490.2 fragment) | Operator and Viewer accounts see the approved unavailable state and receive 403 for admin mutations; denied requests change no protected row. | DB before/after, browser state | `planned` |
| `access.lifecycle.registration-closed` | `integration-access/scenarios/lifecycle-local-bootstrap` (TASK-490.2 fragment) | With `allow_registration = false` and an existing user, a second registration does not create a user. | DB `users` count | `planned` (code reading suggests a defect; not yet executed) |
| `access.lifecycle.oidc` | `integration-access/scenarios/lifecycle-oidc` (TASK-490.6 fragment) | Authorization-code sign-in through a local OIDC provider creates a CF session; group mapping yields the mapped role and environment scope. | Provider and CF DB rows, browser trace | `planned` (needs `services` role) |
| `access.configuration.admin` | `integration-access/scenarios/lifecycle-local-bootstrap` (TASK-490.3 fragment) | Admin users, classification, and retry-policy forms persist and survive reload; cancel discards; a failed save keeps the draft. | DB rows, reload read | `planned` |
| `access.configuration.resources` | `integration-access/scenarios/lifecycle-local-bootstrap` (TASK-490.3 fragment) | Environment create/edit/delete persists through reload; validation errors keep the draft. | DB rows, reload read | `planned` |
| `access.shell` | `integration-access/scenarios/lifecycle-local-bootstrap` (TASK-490.4 fragment) | Navigation reaches every accepted route; unknown routes show NotFound; keyboard focus and sidebar collapse follow the approved Shell reference; unauthenticated routes redirect to `/login`. | Screenshots, visual comparison where an approved state exists | `planned` |
| `access.configuration.preferences` | `integration-access/scenarios/lifecycle-local-bootstrap` (TASK-490.4 fragment) | Profile and notification preferences persist per user and survive reload; one user's change does not affect another. | DB `user_preferences`, reload read | `planned` |
| `access.configuration.notifications` | `integration-access/scenarios/configuration-notifications` (TASK-490.5 fragment) | A real domain event produces an in-app notification; read/dismiss persist; email delivery reaches the local receiver only when enabled. | Receiver log, DB rows | `planned` (needs event source and `services` role) |

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
| `delivery.activation.modes` | TASK-490.14 | `manual`, `auto_latest`, and `pinned` deploy exactly as specified. For BootOnly, before reboot the selected next-boot entry changes, while the running identity and `/run/booted-system` each remain equal to their own separately recorded pre-action values. After reboot, the target boots and runs the selected deployment. A second agent is not affected. | Independently observed boot selection, `/run/current-system`, `/run/booted-system`, DB | `planned` |
| `delivery.activation.recovery` | TASK-490.15 | Rollback restores the previous generation on the target; drift is reported; server, builder, and agent restarts recover in-flight state at the current version. | Target state, DB, journals | `planned` |

### Assurance (`integration-assurance`)

| Contract ID | Scenario | Independent expected result | Evidence | Status |
| --- | --- | --- | --- | --- |
| `assurance.scanning.cve` | `integration-assurance/scenarios/evidence-remediation-lifecycle` (TASK-490.16 fragment) | A real scanner run against the intended closure with deterministic vulnerability data yields the authored CVE set; Current, built, historical, missing, failed, and clean evidence stay distinct in API and UI. | Scanner input data, DB rows, browser state | `planned` |
| `assurance.scanning.hardening` | `integration-assurance/scenarios/evidence-remediation-lifecycle` (TASK-490.17 fragment) | Hardening scan of a real target reports the authored service findings; justification persists. | Target unit facts, DB | `planned` |
| `assurance.policy.enforcement` | `integration-assurance/scenarios/evidence-remediation-lifecycle` (TASK-490.18 fragment) | Policy versions and assignments resolve to the expected effective policies; `Enforce` blocks and `ReportOnly` records without blocking an admission decision. | DB decisions, deployment outcome | `planned` |
| `assurance.policy.stig-import` | `integration-assurance/scenarios/evidence-remediation-lifecycle` (TASK-490.19 fragment) | Importing the DISA Anduril NixOS STIG XCCDF yields the authored rule count and identities; preview does not persist; malformed input is rejected atomically. | DB rows before/after, browser state | `planned` |
| `assurance.remediation.triage` | `integration-assurance/scenarios/evidence-remediation-lifecycle` (TASK-490.20 fragment) | Single and batch triage persist actor, scope, status, and justification for exactly the authored findings; unauthorized triage changes nothing. | DB before/after, browser identity/navigation | `planned`; generate real finding evidence earlier in this scenario |
| `assurance.remediation.finding-waiver` | `integration-assurance/scenarios/evidence-remediation-lifecycle` (TASK-490.27 fragment) | Approved finding-waiver decisions remain distinct from remediation and Current evidence. | DB decision/evidence rows, browser | `planned`; authority may block exact states |
| `assurance.remediation.acceptance-register` | `integration-assurance/scenarios/evidence-remediation-lifecycle` (TASK-490.28 fragment) | The register selects exact source-owned acceptance records; renew/convert affects only the selected record and preserves source identity. | DB before/after, download/list and navigation | `planned`; create source record earlier in this scenario |
| `assurance.remediation.poam-plan` | `integration-assurance/scenarios/evidence-remediation-lifecycle` (TASK-490.21 fragment) | A POA&M created from real failed evidence retains exact finding identity; notes, milestones, and accepted plan transitions persist. | DB rows, browser reload | `planned`; generate failed evidence earlier in this scenario |
| `assurance.remediation.poam-verification` | `integration-assurance/scenarios/evidence-remediation-lifecycle` (TASK-490.29 fragment) | Verification is denied before Current evidence changes; after real remediation, authorized verification/closure/reopen preserves evidence identity and rejects stale writes. | DB state/history, actual target/scanner state, browser | `planned`; construct plan and both evidence states earlier in this scenario |
| `assurance.interchange` | `integration-assurance/scenarios/evidence-remediation-lifecycle` (TASK-490.22 fragment) | Downloaded register (CSV, XLSX, OSCAL JSON/XML), POA&M, acceptance, CVE CSV, XCCDF, and policy-interchange exports parse independently and contain the expected records and scope. | Downloaded files, schema validation | `planned`; export records produced earlier in this scenario |

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

## Product defects and design gaps

| ID | Kind | Summary | Contract | Task |
| --- | --- | --- | --- | --- |
| P1 | Historic product configuration mismatch; repair integrated, behavior unverified | At `fc930335`, the normal module's local selection was missing from TOML and the server unit had `AUTH_MODE=unset`; no auth-mode HTTP result was reached. TASK-490.26 now sets the unit variable from the module option for local and OIDC; targeted Nix evaluation returns both values. The combined runtime proof remains pending. | `access.lifecycle.local-bootstrap` | TASK-490.26 (`136348e8`) |
| P2 | Historic product defect; repair integrated, behavior unverified | At `fc930335`, `/run/postgresql` serialized as an empty-authority URI and server startup failed before migrations. TASK-490.25 now percent-encodes socket host and URI components in active cf-config `DatabaseConfig::to_url`; real parsers from tokio_postgres and SQLx round-trip the socket and encoded values. The combined runtime proof remains pending. | `access.lifecycle.local-bootstrap` | TASK-490.25 (`ca46cdd5`) |
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
| CI `web-ui-baseline-candidates` | Manual visual-maintenance job; not a blocking integration gate | `keep` during coexistence; retire or explicitly rebind at cutover | It produces candidate baselines for legacy `web-ui`; it does not establish a passing integration owner. |
| CI `web-ui-screenshots-mr-comment` | MR artifact/reporting job; not a gate | `keep` during coexistence; update/retire with `web-ui-check` at cutover | Its current screenshot source is legacy `web-ui-check`; replacement owner artifacts need an explicit coordinator update. |

At cutover (TASK-490.24), after all required owner scenarios pass, the integration CI entry point runs the four owners and checks the four-name allowlist; replaced legacy registrations, their CI selections, and the legacy `web-ui` fixtures (`checks/web-ui/tests/fixtures/*.xml` move beneath `checks/integration-assurance/`) are removed in the same change.

## Foundation execution record

Historical bootstrap execution failed at `fc930335878279ac426510359c039dfd8961dc13` (TASK-490.1 foundation plus cherry-picked TASK-491 `af39284bdf16d56402f01a64c916f17948e43b02`). Exact command: `nix build .#checks.x86_64-linux.integration-access.scenarios.lifecycle-local-bootstrap -L`. The Nix build failed before browser launch. Scenario output: `/nix/store/1d5178gxpdv7j4d20dddddr7ik0pq0p3-vm-test-run-cf-it-access-lifecycle-local-bootstrap-run/`. `result.json` reports the required `fresh-startup-applies-migrations` case failed at `controller.wait_for_unit("crystal-forge-server.service", timeout=5)` after a bounded observation found the server restarting. PostgreSQL started and listened on `/run/postgresql`; the server repeatedly exited with `Error: invalid configuration`, caused by `both host and hostaddr are missing` (journal at `journals/controller/crystal-forge.journal`, lines 72–79). No migration ran and no HTTP/auth/browser assertion executed.

The captured, password-redacted generated configuration is `journals/controller/generated-config-summary.txt`:

```text
[database]
host = "/run/postgresql"
name = "crystal_forge"
password = "[REDACTED]"
port = 5432
user = "crystal_forge"
[server]
systemd AUTH_MODE=unset
```

Expected: the module's documented local socket setup starts the server; the `server.auth_mode = "local"` option selects local auth. At the historical run, the socket path was formatted into a URI with an empty authority host and the systemd environment had no `AUTH_MODE`; auth was not reached. The test keeps its startup assertion and adds no `AUTH_MODE` override. TASK-490.25 and TASK-490.26 are now repaired in separate commits, with targeted tests/evaluation and the normal server package build passing. The focused combined VM rerun is the next verification step; until it passes, this contract remains unverified. TASK-491's package-test prerequisite is resolved.

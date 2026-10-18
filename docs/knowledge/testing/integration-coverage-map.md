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

Current application baseline after rebase: `origin/dev` at `5ae7feab8a7b372e8efe0d2dd21b6f53f0625064`. Overhaul branch: `TASK-490.1-integration-foundation`. Parent task: TASK-490. The earlier `b1095c8f`/`ade5080c` passing runs used the prior `33de73b0` base; the current rebased base was verified separately below.

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

The owner's explicit TASK-490.2 session-proof selection authorizes the five session cases below. The concrete Operator/Viewer admin-denial cases (create, PATCH, and config requests rejected without row changes; no usable Viewer admin mutation in the UI) are unimplemented. They are not blocked as a group. Two decisions remain unresolved, and each blocks only its own case: the exact cross-environment endpoint and scope rule, and the closed-registration policy for an existing user with `allow_registration = false`.

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
| `checks/integration-access/scenarios/fragments/task-490.2-sessions.{py,cjs}` | Selected session cases. The scenario `.nix` appends the Python fragment after bootstrap with `pkgs.writeText` and `builtins.readFile`, and registers the browser fragment as `session-sign-out`. No new DSL, gate, or role. |

Registration is explicit and coordinator-owned; there is no custom discovery or completeness flag. The normal `checks.x86_64-linux.integration-access` check builds every gate in its `scenarios` attribute set. Meaningful WIP checks may be discoverable; only Access currently is. An implementation packet does not automatically get a NixOS test derivation. Compatible packet cases are fragments composed by the coordinator into a shared scenario using the existing Nix, Python, and browser-script structure. Missing obligations remain in this map. Future Delivery, Assurance, and Upgrade scenarios, fixtures, and assets all belong beneath `checks/integration-{delivery,assurance,upgrade}/`. Do not create placeholder `default.nix` files. GitLab runs each implemented owner automatically (today only `integration-access`). The retiring legacy checks are optional manual jobs; see [Current GitLab selection](#current-gitlab-selection). Source retirement waits until all required owner scenarios pass.

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

The current table records intended case composition; only bootstrap and the selected session fragment are implemented and verified. The coordinator owns each scenario's `.nix` composition and explicit owner registration. Packet-owned case fragments have disjoint paths under the owner's `scenarios/fragments/` directory; they do not each define or register a NixOS test derivation.

| Scenario | Packet cases to compose | Why this VM boundary is retained |
| --- | --- | --- |
| `integration-access/scenarios/lifecycle-local-bootstrap` | TASK-490.1 bootstrap plus local-mode session/CSRF/sign-out, authorization, closed registration, admin/environment forms, shell/navigation, and per-user preferences from TASK-490.2/.3/.4. Each case uses identities and records produced earlier in this same real workflow; generated identities go through receipts. | One local-auth controller configuration and one evolving database support these cases. No separate packet VM is justified. |
| `integration-access/scenarios/configuration-notifications` | TASK-490.5 real event, in-app preference/delivery, and email delivery cases. | Separate from local bootstrap because proving email requires a different machine configuration: notification email enabled with a local receiver, plus a real delivery/build event. Recreate that event in this scenario; TASK-490.11's VM or output is not reusable. |
| `integration-access/scenarios/lifecycle-oidc` | TASK-490.6 authorization-code, callback, role mapping, and sign-out cases. | Separate because `auth_mode = "oidc"` and the local OIDC provider/redirect setup are incompatible with the local-mode bootstrap proof. |
| `integration-delivery/scenarios/delivery-lifecycle` | TASK-490.8/.9/.10/.11/.12/.13/.14/.15: real ingestion/evaluation, build scheduling/transports, publication variants, activation, rollback, and recovery cases. Use generated commit/build/cache/target identities through receipts. | One controller, service fixture, builder, second agent, and target topology supports the ordered workflows. Keep all produced outputs and target state inside this run. No task-specific Delivery VM is justified by the current module/role requirements. |
| `integration-assurance/scenarios/evidence-remediation-lifecycle` | TASK-490.16/.17/.18/.19 scanning, target, policy, and import cases followed by TASK-490.20/.21/.27/.28/.29 triage, waiver, acceptance, POA&M, verification/closure/reopen, then TASK-490.22 exports of records created earlier in the same scenario. | One controller/target/service configuration and a single actual finding/evidence chain can support these ordered cases. Keep one fresh VM/database for the whole scenario; use receipts for finding, acceptance, plan, and target identities. Do not create packet-specific derivations or depend on another scenario's outputs. Reconsider only if a proven incompatible machine configuration or isolation invariant appears, and record that concrete reason here before splitting. |
| `integration-upgrade/scenarios/version-transition` | TASK-490.23 old-version seed workflow, current-version migration, continuity and recovery cases. | Separate because it intentionally changes the running server version and migrates an in-place database; that compatibility/migration state must not contaminate current-version Delivery or Assurance evidence. Old and current phases remain sequential inside its own fresh VM/database. |

The planned scenario derivations are the local Access lifecycle, notification delivery, OIDC lifecycle, Delivery lifecycle, Assurance evidence/remediation lifecycle, and Upgrade version transition. Their boundaries reflect incompatible auth/service configuration, separate owner contracts, or deliberate in-place version/database transition—not one VM per implementation packet. No independent scenario may use another scenario's rows, sessions, receipts, generations, or output.

Focused verification attributes are concrete even before their scenario files are implemented. Only the local-bootstrap attribute currently exists; the remaining targets below are planned and MUST be registered by the coordinator before execution. Each focused gate has the normal owner aggregate shown after the semicolon.

| Scenario | Focused command; owner aggregate |
| --- | --- |
| `integration-access/scenarios/lifecycle-local-bootstrap` | `nix build .#checks.x86_64-linux.integration-access.scenarios.lifecycle-local-bootstrap -L`; `nix build .#checks.x86_64-linux.integration-access -L` |
| `integration-access/scenarios/configuration-notifications` | `nix build .#checks.x86_64-linux.integration-access.scenarios.configuration-notifications -L`; `nix build .#checks.x86_64-linux.integration-access -L` |
| `integration-access/scenarios/lifecycle-oidc` | `nix build .#checks.x86_64-linux.integration-access.scenarios.lifecycle-oidc -L`; `nix build .#checks.x86_64-linux.integration-access -L` |
| `integration-delivery/scenarios/delivery-lifecycle` | `nix build .#checks.x86_64-linux.integration-delivery.scenarios.delivery-lifecycle -L`; `nix build .#checks.x86_64-linux.integration-delivery -L` |
| `integration-assurance/scenarios/evidence-remediation-lifecycle` | `nix build .#checks.x86_64-linux.integration-assurance.scenarios.evidence-remediation-lifecycle -L`; `nix build .#checks.x86_64-linux.integration-assurance -L` |
| `integration-upgrade/scenarios/version-transition` | `nix build .#checks.x86_64-linux.integration-upgrade.scenarios.version-transition -L`; `nix build .#checks.x86_64-linux.integration-upgrade -L` |

### Access (`integration-access`)

| Contract ID | Scenario | Independent expected result | Evidence | Status |
| --- | --- | --- | --- | --- |
| `access.lifecycle.local-bootstrap` | `integration-access/scenarios/lifecycle-local-bootstrap` | Module `auth_mode = "local"` yields local mode; migrations applied = forward migration files in source; application-role client backends use Unix sockets; first registrant persists with only `admin`; one session after sign-in survives reload; wrong password returns 401 with no session row or cookie. | DB rows, browser result, reload screenshot, generated-config summary, filtered `pg_stat_activity` | `covered` at `81d65be745cc9f9777d368900d01fe0f207cb052`; original five cases unchanged, 292 migrations, 22 socket clients |
| `access.lifecycle.sessions` | `integration-access/scenarios/lifecycle-local-bootstrap`; `scenarios/fragments/task-490.2-sessions.{py,cjs}` | Owner-selected proof: missing/mismatched CSRF on DELETE returns 403 with session lifecycle rows unchanged; valid CSRF returns 204 and invalidates only the selected row; its next GET returns 401 while the retained session returns 200. Independent browser profile Sign out returns logout 204, persists exact-row invalidation, survives reload at `/login`, rejects protected GET with 401, and redirects protected navigation to `/login`. | Exact `user_sessions` before/after, retained/revoked/browser receipts, browser result and screenshot | `covered` at `81d65be745cc9f9777d368900d01fe0f207cb052`; five appended cases passed under the owner's session selection |
| `access.lifecycle.authorization` | `integration-access/scenarios/lifecycle-local-bootstrap` (remaining TASK-490.2 cases) | Operator/Viewer admin create, PATCH, and config requests are rejected without row changes, and Viewer has no usable admin mutation in the UI. Cross-environment denials need the exact endpoint and scope rule (pending D1). No protected write on denial. | DB before/after, browser state | `planned`: Operator/Viewer cases unimplemented. `blocked: decision`: only the exact environment-scope cases |
| `access.lifecycle.registration-closed` | `integration-access/scenarios/lifecycle-local-bootstrap` (remaining TASK-490.2 cases) | Pending D1: with `allow_registration = false` and an existing user, a second registration does not create a user. | DB `users` count | `blocked: decision`; case unimplemented (code reading suggests a defect; not executed) |
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
| P1 | Historic product configuration mismatch; repair integrated and verified in the Access VM | At `fc930335`, the normal module's local selection was missing from TOML and the server unit had `AUTH_MODE=unset`; auth was not reached. TASK-490.26 now sets the unit variable from the module option for local and OIDC. NixOS evaluation and the rebased Access runtime setup-status/browser flow verify local mode; OIDC option evaluation verifies its unit value. | `access.lifecycle.local-bootstrap` | TASK-490.26 (`76e51464`) |
| P2 | Historic product defect; repair integrated and verified in the Access VM | At `fc930335`, `/run/postgresql` serialized as an empty-authority URI and server startup failed before migrations. TASK-490.25 now percent-encodes socket host and URI components in active cf-config `DatabaseConfig::to_url`; both client parsers round-trip socket and encoded values, and the rebased Access VM starts through the production local socket and applies migrations. | `access.lifecycle.local-bootstrap` | TASK-490.25 (`52128b3a`) |
| P3 | Independent incidental product defect; not an Access gate failure | On fresh boot the `crystal-forge-postgres-jobs` oneshot races database-role setup and logs `FATAL: role "crystal_forge" does not exist`. The setup script creates the role later; server startup, migrations and all Access cases pass. This is not authorized for repair in this overhaul. | VM journal; `access.lifecycle.local-bootstrap` still passes | TASK-492 |
| G1 | Design gap | No approved design component for `/login`, `/register`, or `/setup`. | `access.lifecycle` visual cases | None; semantic assertions only |

## Legacy retirement map

Disposition values: `replace` (retire after the required owner scenarios pass), `keep` (non-integration check; keeps its own attribute), `duplicate`, `obsolete`. A disposition describes final source retirement. It does not describe the current GitLab selection, which is a separate decision recorded in [Current GitLab selection](#current-gitlab-selection).

Policy: implemented integration owners run automatically in GitLab. Retiring legacy integration checks remain available for optional manual execution while replacements are constructed. Removing a legacy check from automatic CI does not mark its obligations covered. Delete its definition and fixtures after passing replacement coverage is recorded, or after an explicit decision removes the underlying requirement.

| Legacy check or job | Kind | Disposition | Replacement owner / scenario or reason |
| --- | --- | --- | --- |
| `checks/web-ui` (CI `web-ui-check`, manual, allow_failure) | Integration, browser | `replace` | Access, Delivery, Assurance browser scenarios. 105 of 181 steps use route mocks; mocked steps are not carried forward as integration proof. Authentic real-API steps (03/05 registration and login, 13h force-push, 20aa/20ab/20ad/20af, 21–25, 29g–29m, 30b/30d/30e) map to `access.*`, `delivery.evaluation.ingestion`, `delivery.publication.*`, `assurance.*`. |
| `checks/niks3-ui` (CI manual) | Integration | `duplicate` | Re-runs web-ui step 25 and export phases; replaced by `delivery.publication.niks3`. |
| `checks/web-ui-reconciliation` | Browser, fully mocked | `obsolete` | Every API response mocked; replaced by `assurance.policy.stig-import`. |
| `checks/integration` (CI manual) | Integration | `replace` | `delivery.evaluation.ingestion` (polling, webhook, branches), `delivery.activation.*` (agent accept, desired target). Grafana and SQL-view assertions are not product integration obligations (Grafana is optional/legacy). |
| `checks/oidc-auth` (CI manual) | Integration, shallow | `replace` | `access.lifecycle.oidc` (adds callback, session, mapping). |
| `checks/niks3-cache` (CI manual) | Integration | `replace` | `delivery.publication.niks3`, `delivery.build.strategies`. Seeded queue rows are replaced by real evaluation. |
| `checks/niks3-write-probe` (CI manual) | Integration | `replace` | `delivery.publication.niks3` (connection Test boundary). |
| `checks/attic-metrics` (CI manual) | Integration | `replace` | `delivery.publication.variants` (Attic). |
| `checks/attic-publication` (CI manual) | Component | `replace` | `delivery.publication.variants` (Attic publication scope). |
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
| CI `web-ui-screenshots-mr-comment` | MR artifact/reporting job; not a gate | `obsolete`; removed from `.gitlab-ci.yml` by the TASK-490.24 early transition | Its only screenshot source was `web-ui-check`, which is now manual. The manual job keeps its downloadable artifacts, and `integration-access` exports its own evidence. No replacement comment bot exists. |

### Current GitLab selection

The TASK-490.24 early transition changed only the GitLab selection. It deleted no legacy check definition or fixture, and it marked no legacy obligation covered.

| GitLab job | Selection now | Notes |
| --- | --- | --- |
| `integration-access` | Automatic and required on merge requests and `main` | Builds the normal owner aggregate `checks.x86_64-linux.integration-access`. Exports `integration-evidence/integration-access/` even when the build fails. |
| `legacy-flake-check: [integration]`, `[oidc-auth]`, `[niks3-cache]`, `[niks3-write-probe]`, `[niks3-ui]`, `[attic-metrics]`, `[attic-publication]` | Manual, `allow_failure: true` | Same `flake-check` definition and commands. They were `flake-check: [<name>]` before. |
| `web-ui-check` | Manual, `allow_failure: true` | Same command and artifacts. |
| `web-ui-baseline-candidates` | Manual, `allow_failure: true` | Unchanged. |
| `flake-check: [run-ui-dev-db-check]`, `[server-regressions]`, `[nixos-discovery-reliability]`, `[builder-evaluator-packaging]`, `[web-ui-test-runner]`, `[okf-knowledge]` | Automatic | Non-integration checks; unchanged. |
| `cve-processing-test`, `dashboard-visibility-tests`, `state-machine-tests` | Automatic | Disposable-database and `--ignored` Rust tests; a package build does not replace them. |
| `coverage-check`, `complexity-check`, `auto-bump-patch` | Unchanged | Release and version behavior is unchanged. |

Automatic integration coverage is now exactly the ten Access cases: five bootstrap cases and five session cases. These obligations have **no automatic integration coverage** until their replacements exist. A person must start the matching manual legacy job to run them:

- Access: authorization, registration policy, OIDC, forms, preferences, notifications, and shell behavior. The legacy `oidc-auth` and `web-ui` checks cover part of this.
- Browser and design breadth that the legacy `web-ui` check exercises.
- All Delivery obligations: ingestion, builder strategies, Niks3 and Attic publication, activation, rollback.
- All Assurance obligations.
- All Upgrade obligations.

This is a deliberate, temporary reduction in automatic coverage. It does not change the dispositions above. Final retirement of a legacy check needs a recorded passing replacement or an explicit decision that removes the requirement.

At final cutover (the remaining TASK-490.24 work), after all required owner scenarios pass, the integration CI entry point runs the four owners and checks the four-name allowlist; replaced legacy registrations, their CI selections, and the legacy `web-ui` fixtures (`checks/web-ui/tests/fixtures/*.xml` move beneath `checks/integration-assurance/`) are removed in the same change.

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

Expected: the module's documented local socket setup starts the server; `server.auth_mode = "local"` selects local auth. The historical `fc930335` run failed before migrations because the socket URI and unit mode were invalid. TASK-490.25/.26 repaired those issues in separate commits. The scenario passed before rebase at `b1095c8f` and `ade5080c`, then after rebase at `bba718befbc183863781656df2df272e5f735d5a` with five bootstrap cases. That earlier rebased evidence remains at `/nix/store/riky1mm0nn185wb26abw7zzshnbml0fp-vm-test-run-cf-it-access-lifecycle-local-bootstrap-run/`.

### Current bootstrap and selected session proof

Tested source: `81d65be745cc9f9777d368900d01fe0f207cb052`. Both commands passed:

```bash
nix build .#checks.x86_64-linux.integration-access.scenarios.lifecycle-local-bootstrap -L
nix build .#checks.x86_64-linux.integration-access -L
```

The focused gate passed ten cases: the original five bootstrap cases, followed by missing-CSRF denial, mismatched-CSRF denial, exact selected-session revocation, revoked-request denial with retained-session success, and independent browser sign-out. The owner aggregate reused this run without another VM. It proves the registered subset, not every Access obligation or the other owners.

Evidence: `/nix/store/i84kvs1lc5nqx0lmsqinwbmvy0ix8mvh-vm-test-run-cf-it-access-lifecycle-local-bootstrap-run/`. Its actual `result.json` reports `status: passed`, no error or faults, and these receipts:

| Receipt | Recorded value |
| --- | --- |
| `migrations.applied` | `292` |
| `database.unixSocketClients` | `22` |
| `admin.userId` | `0560a10d-20c1-4464-a174-9b1e1eddbaf4` |
| `sessions.retainedId` | `4734fef6-c3d6-43ee-9a5d-9f2bc66305b0` |
| `sessions.revokedId` | `9a4e59eb-450b-4105-b54b-6514ba9d307a` |
| `sessions.browserSignOutId` | `ad65734a-ba57-4935-9273-a70a987b749b` |
| `sessions.revocationPersisted` | `true` |
| `sessions.browserSignOutPersisted` | `true` |

The socket assertion observes `pg_stat_activity` filtered by the configured database, configured role, and `backend_type = 'client backend'`. It requires a positive count and every observed connection to have `client_addr IS NULL` and `client_port = -1`; configuration text alone is not socket proof. The SQLx parser table now checks the password recovered from parsed options via `to_url_lossy()` authority, removes SQLx-specific query options, and parses with `tokio_postgres`. The reported targeted parser command passed 2/2 tests; this documentation follow-up does not rerun it.

Reviewer artifacts:

- [Scenario result](https://gitlab.com/-/project/70402481/uploads/2ab7a8febcfb1aa1386364464decc2f2/result.json)
- [Sign-out browser result](https://gitlab.com/-/project/70402481/uploads/5d551029e97b8b3dd534f312deb35b5f/result.json)
- [Signed-out protected navigation screenshot](https://gitlab.com/-/project/70402481/uploads/ec258b80b8a107c27d32b9f5c4cf3109/signed-out-protected-navigation.png)

The browser result reports `passed`, the exact browser-session receipt above, final URL `http://localhost:3000/login`, and no visual comparisons. The first session attempt at `537186835956d7af48a0868289e16364625dd85c` failed the last UI case because of the coach overlay and unhandled-promise test mechanics. The correction closes the coach through its real control and coordinates the click/response with `Promise.all`; no assertion was weakened. Failed evidence remains at `/nix/store/i5kgnb35xbywm8wj0k7618n90vhg8swl-vm-test-run-cf-it-access-lifecycle-local-bootstrap-run/`. Do not upload its logs containing a runtime token; the corrected header stays in a VM file.

TASK-490.1 foundation is in `Review`; all seven bounded foundation criteria and documentation validation passed. TASK-490.2 remains `In Progress`: Operator/Viewer cases are unimplemented; exact cross-environment scope and closed-registration policy cases await their accepted decisions. These documentation-only follow-ups occur after the tested source and do not require another VM run.

All runtime journals record a separate `crystal-forge-postgres-jobs.service` first-boot failure because its configured role is not created yet. The module setup creates the role shortly afterward, so the server and Access cases succeed. TASK-492 records this unrelated startup-order defect; it is outside the authorization for TASK-490.25/.26 and is not an Access gate assertion. Do not claim all Access obligations or the other owners are covered by this bootstrap slice.

### Access through the GitLab job script

Tested source: `2c494ac4bf8cc7155899723ee69fcf879ac49248`, the TASK-490.24 early CI transition on a clean tree. The exact script that the `integration-access` job runs:

```bash
sh packages/ci/integration-owner-build.sh integration-access
```

It builds the normal owner aggregate through `packages/ci/public-cache-build.sh` and exited 0. This run **executed the VM**; it did not reuse an earlier result. The source revision changed, so Nix built 24 derivations including the scenario `vm-test-run` derivation. The driver log records `source 2c494ac4…`. The whole job took about 49 seconds because the production packages were already in the Nix store. The scenario recorded 17.6 seconds of case time.

The scenario `result.json` reports `status: passed` with the same ten cases as above, 292 migrations, and fresh runtime identities. `database.unixSocketClients` was 25 in this run, and `admin.userId` was `893003e2-edb7-4513-a6c7-f2f2e44d451f`. Both browser results report `passed`. The owner aggregate is `/nix/store/7yijp3qqr4iw6zk2zq1dps1r6f6wqrp7-integration-access`. Its run output is `/nix/store/cmvnmvc7sbdhd64vj9rd0yqz0iq9lzac-vm-test-run-cf-it-access-lifecycle-local-bootstrap-run`.

The exported workspace directory `integration-evidence/integration-access/` held 994 KB of real files: `nix-build.log` and, under `lifecycle-local-bootstrap/`, `result.json`, `meta.json`, both browser results, both screenshots, console logs, and the controller journals. A search found no real runtime token in the build log or evidence. The only inline `x-csrf-token` value is the deliberately fake mismatched-token literal; the real header stays in a VM file. The script also passed three local exercises with a `nix` shim: a failed build keeps its exit status, copies only run outputs that already exist, and never builds a `.run` derivation; a successful build with a missing evidence link exits 1.

This record covers only the CI path for the Access subset. It is not evidence for the legacy checks' obligations or for any other owner.

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

## Executed connected Assurance increment

At `9f53fade-dirty` on x86_64-linux, the normal owner commands executed fresh
VMs: Access passed 23/23 (driver 51.13 seconds), Delivery passed 15/15 (400.00
seconds), and Assurance recorded 44 passing case records and two mandatory
failures (266.51 seconds). Assurance has 40 distinct case names: nested Current
and continuity checks repeat names. These are not 46 independent requirements.
Command totals were 60.93, 817.19, and 695.96 seconds respectively.

| Owner | Retained run output prefix | Outcome |
| --- | --- | --- |
| Access | `91fg9zs3b4x63jxn461fiwa4k5rf2yvg` | Passed selected session, authorization and form coverage. |
| Delivery | `pkka8ycywlvpsm1z1aj7h8nj6psnbba0` | Passed real A/B deployment, complete History membership, retained rollback and exact closure acquisition. |
| Assurance | `bfmn1qp6285mdxlw76iai3c6b2267648` | Failed historical cell copy and false-clean failed-scan summary; independent recovery cases executed. |

Each prefix identifies its `vm-test-run-cf-it-<owner>-<scenario>-run` output.
Collected results, metadata, screenshots and journals are under
`integration-evidence/<owner>/<scenario>/`. Local results remain dirty-input
evidence after publication; hosted CI must verify the committed inputs.

Assurance authors its own source and runs native evaluation, API-only builder
build/publication, packaged vulnix with a trusted local HTTPS NVD feed, sealed
schema-1 ingestion, actual UI deployment and real agent observation. It proves
missing-before-build-admission, affected Current, clean built-not-deployed,
corrected deployed Current, and preserved historical observations. Remote
evidence is explicitly `unverified_remote`, not server-local verification.
Real host Schedule patch, plan edits/note/milestone, scoped Viewer denials,
explicit verification and authoritative closure preserve the original finding,
link and complete opened-against baseline. Plan/verification mutations use real
APIs; the browser proves their resulting detail and reload, not every edit
control. System triage creation itself uses the real browser workflow.

Valid upstream JSON with CVSS 11 causes deterministic result rejection. Invalid
JSON causes transient requeue; restoring the normal feed produces a new
execution and completed source without changing corrected Current. Both faults
use real replacement-build post-build leases. Manual retry admission is tested
separately; manual execution on a remote-only topology is not proved.

At the historical `9f53fade-dirty` checkpoint, the approved `SystemDetail.jsx`
`CvesTab` cell-copy assertion failed: production rendered `Historical inventory`
instead of `read-only`. Mutation controls were absent and API authority was
correct, but the copy failure prevented historical reload. The second failure
rejected `0 vulnerabilities across 0 packages` in failed scan Details without a
completed finding set. Both failures remain in that checkpoint's evidence.
The user subsequently authorized exactly two presentation repairs: the historical
Legacy row's `read-only` tooltip chip, and omission of unsupported Findings.
Both repairs passed the normal Assurance owner at clean committed `5ef2d0b0`.
Loading, unavailable, clean-empty, GET error/Retry, Viewer and failed-scan states
have actual browser artifacts. Screenshots were inspected; this is not broad
visual parity or scale/performance proof. Reopen, recurrence, batch isolation,
waiver/acceptance, policy, hardening and interchange remain open.

Checkpoint `204a4752` hosted pipeline `2970142621` failed: Delivery 9 passed/1
failed at unsupported `nix-store --query --all`, before acquisition/deployment;
Access passed 23, state-machine passed 525/0/0 including eight native-authority
regressions, and core/generated-TOML/warm-Nix tests passed. Component then failed
the missing CVE fixture manifest entry. CVE/dashboard job status was green but
`initdb` failed as root and no tests ran. Complexity and coverage were skipped.
Those statuses are not passing regressions. The subsequent test increment
corrected the flag, retained exact closure membership without oversized JSON
transport, and added the missing documentation-manifest entry without a product
repair. The later authorized presentation repairs are recorded separately below.

Current native inventory is Access, Delivery and Assurance. Upgrade has no
placeholder: D2 still requires an approved predecessor and compatibility window.

### Hosted checkpoint and presentation repair verification

[Pipeline 2971039033](https://gitlab.com/crystal-forge/crystal-forge/-/pipelines/2971039033)
failed at exact revision `3ec99f7273736e69df9c4918727f8328db1cecb1`.
The 2026-10-21 single-snapshot audit is retained at
`/tmp/opencode/pipeline-2971039033/report.md`.

| Job | Audited result |
| --- | --- |
| Access / Delivery | Fresh owner executions passed 23/23 and 15/15 respectively. |
| Assurance | 40 passed / 2 failed case records: 42 records, 37 distinct names. Historical `read-only` mismatch and failed-scan false-clean summary failed again. |
| State machine | Actual DB execution: 525 passed, 0 failed, 0 ignored across 237 Rust summaries; migration/rehearsal and regression phases exited 0. |
| Component | Actual passing execution: nine Rust summaries, 1,935 passing executions, 0 failures, 779 ignored entries. These are execution totals, not unique tests or all DB regressions. |
| CVE / dashboard | Hosted statuses were green, but both launchers failed root `initdb` and ran no Rust tests. These are false-green statuses. |
| Complexity / coverage | Skipped; no measured result. |

After the two retained UI failures, Assurance proved malformed-feed requeue but
aborted when normal-feed restoration found an `in_progress` retry at the
idle-scanner precondition. Recovery and administrator browser retry were not
reached in this hosted run. This active-retry/idle-precondition race does not
prove permanent scanner failure. The earlier `9f53fade-dirty` recovery evidence
above remains valid only for its recorded inputs.

The coordinator added a bounded freeze/thaw idle wait to the existing test
helper, not product code. The first local repair run ended with 44 passed and
one failed case at `3ec99f72-dirty`. Both original presentation cases passed;
an added recovery assertion incorrectly required retained error metadata to be
null. The assertion was corrected to require the completed lifecycle and exact
completion event while preserving the earlier requeue diagnostic. No production
error metadata was changed to satisfy that test.

At clean committed `5ef2d0b0622fd45e59276754c0c7e0f8cd67e030`,
`nix develop --command sh packages/ci/integration-owner-build.sh integration-assurance`
exited 0 with a fresh VM execution: **46 passed case records**, 40 distinct names,
driver 215.53 seconds. The run includes the idle-wait fix. Evidence output:
`nggxx00r5f9dnwm69b3jzzjz1bk7fk99-vm-test-run-cf-it-assurance-evidence-remediation-lifecycle-run`.
Both `result.json` and `meta.json` identify the clean commit. Historical
reload/leave-return, exact chip/title/no-control assertions, completed zero/nonzero
summaries, failed Findings omission with retained diagnostics, and real
recovery/retry admission passed. Full Web UI Rust tests passed 766/0/1 ignored;
the Web UI package build reused its result. This is not a hosted-CI pass.
Pipeline `2972524672` was running at the last snapshot.

The repair [reference/before/after bundle](https://gitlab.com/-/project/70402481/uploads/1e12d6f176a94ed55edb3da0cd22c08b/mr335-assurance-presentation-5ef2d0b0.tar.gz)
and six inline MR335 images retain the approved reference, genuine prior dark
before states, and real clean-commit after states. After captures cover dark and
light at 1440 by 900 pixels, with actor, exact selected evidence and scenario
metadata. Supplementary metadata-end captures record actual clipping checks;
the retained receipt reports all eight captured metadata ends fully visible.
No light-theme before state was available.

TASK-490.20 then resumed without product or fixture changes. The normal owner
passed 47 case records with 41 distinct names at `5ef2d0b0-dirty`, driver 246.38
seconds, evidence `39bxp18w8ap718xx0r6f63h9imv7fcz8`.
The existing host-schedule browser journey now reopens and cancels the exact
hydrated editor twice without an additional remediation write. Complete POA&M
API detail remains unchanged and its exposed baseline agrees with SQL. The
separate valid-CSRF Admin request without POA&M metadata returns
`400 invalid_poam_payload`, preserving nine remediation tables, the original
scan seal and all observations. This is not SQL before/after proof of browser
reopen, successful batch application, or unselected-finding isolation.

Preview: ready (owner-reported), production `http://127.0.0.1:8090/`, API
`http://127.0.0.1:3490/`, isolated task PostgreSQL port `5490`, design
`http://127.0.0.1:39997/crystal-forge.html`. Existing supervisors are
`cf-assurance-production-start` and `cf-assurance-design`; backend freshness
remains with the coordinator. Stock fixtures have no scans or historical
selections, so they cannot demonstrate the populated repairs. No scan results
were seeded. Genuine before evidence remains dark-theme `9f53fade-dirty` VM
captures; new reference captures cover both themes. Live empty-state captures
are not populated-state verification. Inspection and capture paths remain in
`/tmp/opencode/cf-assurance-browser-inspection.md`; references are unchanged.

### Selected browser design review

Reference components were inspected at tree `204a4752`, with the selected
reference contracts at `6e4ea520`: `SystemDetail.jsx` `CvesTab` line 1825
specifies the `read-only` chip (`chip chip-unknown`, title
`This selected scan cannot authorize triage`); `ScanningView.jsx`
`ScanLogDrawer` uses `cfg.found` to omit Findings without a completed set.
The approved repair adds no unavailable replacement banner. SC1 revision 3 and the
normative continuity contract control evidence and permission meaning. The
continuity specification retains precedence for Current authority. The following
whole-region gaps carry no authorization for additional product repairs.

| Surface/state | Actual observation and remaining difference |
| --- | --- |
| System CVEs Current | Package-first sections, automatic expansion, exact finding and single triage execute. Reference batch quick-picks and batch bar are not implemented; multi-selection remains open. |
| Exact scan Details | Real scan/job/execution identity and clean versus affected states execute. Failed Findings omission passed at clean `5ef2d0b0`, with failure context and all metadata retained. Completed Findings-first/inset-card structure still differs from the reference. Freshness and CVE database are absent; several execution timestamps replace Last scan. Configuration/Revision labels and order differ from Config/Commit. Extra execution diagnostics do not establish reference parity. Scanner renders duplicated `vulnix vulnix 1.12.4`; the version difference itself is fixture data. Details-level View these CVEs remains absent. |
| Host Schedule patch | Browser creates the real host plan and reloads Scheduled. The subsequent `5ef2d0b0-dirty` run reopens/cancels twice, checks hydration and no additional remediation writes, and retains two dialog images. This is not full dialog parity. Reference Exploited context and no-upstream-patch warning remain gaps. |
| Maintained POA&M | Actual metadata, plan, note, milestone and baseline persist. The browser reads/reloads API-authored changes. Visible reference lifecycle explanation/action and scope severity/fix/environment/commit metadata differ; lower activity outside the viewport is not called missing. |
| Closed POA&M | Two sealed PASS attempts and original retired baseline history are visible. Large verification cards replace the compact reference confirmation and push baseline history lower. A readable baseline/Current comparison and all required metadata are not fully proven by the image. Verify/Close/Reopen browser clicks remain unexecuted. |
| Historical | Explicit selection, original sealed source and no mutation control are proven. The original cell-copy failure is preserved as historical evidence. Approved tooltip-chip rendering and reload/leave-return assertions passed at clean `5ef2d0b0` in both themes. Generation versus commit selection and fixture values are context differences; extra package-load metadata is an observed difference, not an authorized removal. |
| Missing/clean/loading | The tab distinguishes no completed scan, completed-empty and loading without enqueuing. Global green CVEs 0 above unavailable inventory remains a header/count consistency gap. |
| Error/Retry | Held/aborted real GET exercises loading/error; restored transport supplies the real retry result without API writes. Raw stack text dominates the error frame and pushes Retry below the viewport, unlike the bounded reference message. |
| Fleet drawer/Viewer | Exact fleet-to-Current navigation and Viewer no-control behavior execute. No drawer PNG was retained; source-only differences include Published versus Discovered, missing fleet denominator, and tier-first rather than environment-first grouping. Viewer-specific disposition copy lacks a complete approved-state comparison. |

One package/finding on desktop does not prove pagination, density, responsive
layout, focus/keyboard behavior, draft preservation across scope changes, or
query-scale performance. Fault/recovery evidence does not independently retain
every live lease-owner field; initial completion has the stricter full lease
assertions. These are unfinished verification obligations, not authority for
unrelated product edits.

## Connected Delivery repair milestone

The local `01971f7f-dirty` run passed all 15 cases in the normal Delivery
aggregate. The driver took 463.44 seconds; the complete command took 882.70
seconds. This remains a dirty-tree result, not a clean-commit CI pass. Evidence:
`y93xqisak2bb96sxq7g9hpk1ran04i3f-vm-test-run-cf-it-delivery-delivery-lifecycle-run`.

The run built and published A and B, activated each through the production UI,
and restored A through retained-generation History rollback. Actual generations
were 1, 2, and 3; state rows were 2, 3, and 4. Each state had two subsequent
correlated heartbeats. Started cards remained nonterminal and the attempt count
was 1, 2, then 3 after reload. This is selected workflow coverage, not broad
visual parity, other activation modes, drift, or restart recovery.

The whole-closure acquisition proof uses an independent empty rooted store
inside targetA, with an unprivileged reader and only CF publisher trust. The
ordinary deployment store has prepared runtime prerequisites, but each runtime
candidate is absent before acquisition. The separate signature receiver is an
enforcement control, not the source of CF publisher trust.

Separate product repairs: TASK-497 (`6a1bf542`, `6e131456`) fixes generated
polling-duration parsing; TASK-498 (`18ef4274`, `3d866ebf`) fixes recursive
publication signing/trust propagation; TASK-499 (`62e86efa`, `01971f7f`)
supplies typed native retained authority and rejects retrospective/incomplete
lineage; TASK-500 (`594d6a12`) fixes Started presentation, attempt counting,
and the approved `switch_to_generation` label. Normal component checks include
generated TOML and warm-store trust tests. The isolated database runner now
explicitly selects the native authority negatives.

Publication review strengthens exact closure-manifest retention and whole
rendered History membership. These additional assertions require their own
committed-input hosted execution. Historical failures below remain valid for
their recorded inputs. Assurance and Upgrade are not proved by this milestone.

This map is maintained under the [integration test maintenance contract](integration-test-contract.md). Its product inventory starts from supported application boundaries and accepted requirements, not the retired tests. A source route, component, test function, or planned scenario is a discovery fact, not coverage. Twenty-three Access cases passed again after semantic source cleanup at `590ee94c635154e027317414298ebe05e9b48bbd`: five bootstrap, five session, six authorization and seven Admin-form cases. The inventory below adds no covered behavior.

Current application baseline after rebase: `origin/dev` at `5ae7feab8a7b372e8efe0d2dd21b6f53f0625064`. Overhaul branch: `TASK-490.1-integration-foundation`. Parent task: TASK-490. The earlier `b1095c8f`/`ade5080c` passing runs used the prior `33de73b0` base; the current rebased base was verified separately below.

Status values:

| Status | Meaning |
| --- | --- |
| `Integration-covered` | The named subset executed and passed at the recorded revision; no implied coverage of adjacent operations. |
| `lower-level-covered` | A named Rust, component, schema, or isolated-DB assertion has recorded passing evidence. Never integration proof. No new row receives this status in this research pass. |
| `planned` | Required or discovered obligation with an intended owner/scenario; not integration-proven. Tracking marked **unassigned** still needs explicit coordinator selection. Existing but unexecuted assertions also remain planned here. |
| `failing` | An executed correct assertion fails at a recorded revision; link its defect and evidence. Historical failures remain in the execution record. |
| `blocked: <decision ID>` | Only the stated case cannot receive an independent expectation until that exact decision or design state is supplied. Other cases in the family proceed. |
| `out of scope` | A named unsupported, developer-only, or retired behavior has no product integration obligation under the accepted contract. |

## Authority and case-local decisions

Most current behavior is described by `docs/knowledge/` concepts with `implementation_status: implemented`. Those documents describe as-built behavior; the contract says as-built behavior does not approve itself. Approved design authority exists for Systems/SC1 (`systems-view-design-v0.1.md` Section 22 owner decisions) and the owner-approved CVE/POA&M continuity specification. The compliance, CVEs, cross-view, and XCCDF design documents are review drafts.

There is no global D1 approval gate on the product inventory. Implemented knowledge concepts locate mechanisms; their status does not approve every implementation choice. Review drafts also do not approve themselves. The adopted maintenance contract assigns the supported boundaries to owners. The active child-task selections, approved design states, Systems owner decisions, and normative CVE continuity specification supply independent expectations within their scopes. Detailed new expectations must name the applicable accepted clause before execution. A conflict blocks its specific case, not its entire family. A `planned` discovery row is an explicit unproved obligation, not ratification of as-built behavior. An accepted expectation wins when code differs.

The owner's explicit TASK-490.2 selections authorize the five session cases and the Operator/Viewer admin-denial cases below. The admin-denial cases (create, PATCH, and configuration requests rejected with 403 and no protected change; no usable Viewer admin mutation in the UI) are implemented and executed. They rest on the approved design's role catalogue ("Manage users & OIDC", "Edit server config" belong to Admin) and its "Admin-only" Server Management page; they do not define Operator as generally read-only. For the remaining TASK-490.2 cases, two decisions remain unresolved: the exact cross-environment endpoint and scope rule, and the closed-registration policy for an existing user with `allow_registration = false`. Each blocks only its own case. Neither case is implemented.

| Decision | Exact missing fact and affected cases | Work that can proceed |
| --- | --- | --- |
| D1-scope | Select the cross-environment endpoint/method and accepted visibility/mutation rule, including hidden-versus-absent response. The environment handler currently filters membership and returns non-enumerating 404 for a hidden detail; that is evidence, not the owner's selection for TASK-490.2. | Selected admin denials, forms, and already-approved system/CVE non-disclosure checks. |
| D1-registration | With an existing user and `allow_registration = false`, specify registration denial/status and whether first-admin bootstrap is the sole exception. | Fresh bootstrap and ordinary login/session cases. |
| D2-upgrade | Name oldest supported in-place server release, pinned source, and agent/builder mixed-version support window. Tags and crate version strings do not establish support. | Current-version restart recovery and migration/component verification. |
| D3-form-state | Identify approved `AdminView.jsx` / `EnvironmentsView.jsx` state for real failed-save draft retention and any missing fresh-empty, pending, or disabled-save state. | TASK-490.3 accepted success, cancel, duplicate validation, reload and disabled-account login cases. |
| D4-auth-transport | For `POST /webhook`, specify whether unsigned public ingress is accepted or requires authenticated origin, and the rejection/no-enqueue outcome. Separately, specify missing/mismatched-CSRF outcomes for cookie-authenticated `POST /api/v1/admin/users`, `PATCH/DELETE /api/v1/admin/users/:id`, `PUT /api/v1/admin/{classification-config,automatic-retry-policy}`, and `POST /api/v1/environments`, `PATCH/DELETE /api/v1/environments/:id`; these handlers do not call the shared CSRF guard. Do not infer universal 401/403/CSRF policy from routing. | Valid-CSRF role denials already proved; authenticated form success; selected session CSRF cases; builder signatures/job authorization. |
| D5-interchange | Approve the exact draft CF-XCCDF portable identity/digest/trust reconciliation rules and policy-interchange conflict outcomes before making those draft-only details release gates. | Accepted STIG preview/import atomicity, authored rule identities, schema validation, download content/scope. |
| D6-presentation | Supply approved login/register/setup visual references and any missing specific domain UI state. G1 names the auth pages. A missing visual reference does not block semantic API/browser workflows. | Designed interactions and independently specified domain outcomes. |

No unresolved decision is asserted for CVE stable identity, Current versus historical evidence, prospective environment decisions, verification freshness, recurrence, or acceptance renewal/conversion. The normative continuity specification already decides these, including Section 32.

## Harness interface

Shared helpers and registration points:

| Path | Responsibility | Tracking |
| --- | --- | --- |
| `lib/integration-test/default.nix` | Snowfall export: call `lib.crystal-forge.makeIntegrationTestHarness { inherit pkgs inputs; }` from native check arguments. | Foundation |
| `lib/integration-test/harness.nix` | Harness implementation: production `packages`, `roleNames`, and `mkScenario`. | Foundation |
| `lib/integration-test/roles.nix` | Role modules: `common role`, `controller { browser ? false }`, `services {}`, `builder { name, strategy ? "server_derivation" }`, `target {}`. Services supplies local smart-HTTP Git and an authenticated Nix store; OIDC/email/other cache fixtures remain unimplemented. | TASK-490.7 |
| `lib/integration-test/browser.nix` | Render profile `cf-it-render-v1`, `cf-browser` wrapper, browser NixOS module. | Foundation |
| `lib/integration-test/browser/cf-browser.cjs` | Browser spec runner and `ctx` API. | Foundation |
| `lib/integration-test/facts.nix` | `byId`, `localUserFromDesign`, `designReference`, `designSite`. | Foundation |
| `lib/integration-test/driver.py` | Python helpers available in every scenario body. | Foundation |
| `checks/integration-access/default.nix` | Coordinator-owned explicit `scenarios` attribute set; normal check builds all registered gates with `pkgs.linkFarm`. | Foundation |
| `checks/integration-access/scenarios/<name>.{nix,py,cjs}` | One Access scenario. Coordinator owns the scenario `.nix`, Python composition, and explicit registration. Behavior-named Python/browser fragments and fixtures live under `scenarios/fragments/` or `fixtures/`; tracking boundaries do not imply test derivations. | TASK-490.24 composition |
| `checks/integration-access/scenarios/fragments/sessions.{py,cjs}` | Selected session cases. The scenario `.nix` appends the Python fragment after bootstrap with `pkgs.writeText` and `builtins.readFile`, and registers the browser fragment as `session-sign-out`. No new DSL, gate, or role. | TASK-490.2 |
| `checks/integration-access/scenarios/fragments/authorization.{py,cjs}` | Selected Admin-only role denials; exports `authz_state()` for protected-state checks shared with forms. Browser registration: `viewer-admin-denied`. | TASK-490.2 |
| `checks/integration-access/scenarios/fragments/admin-forms.{py,cjs}` | Admin account and settings workflows appended after authorization; browser phases return to Python for independent SQL verification. | TASK-490.3 |

Registration is explicit and coordinator-owned; there is no custom discovery or
completeness flag. Access and Delivery each build every gate in their own
`scenarios` attribute set and run automatically in GitLab. Tracking tasks do not
define source files, VM boundaries or derivations. Compatible behavior modules
compose through the existing Nix, Python and browser scripts. Keep all scenario
assets beneath their functional owner. Assurance now registers the executed
evidence/remediation lifecycle. Upgrade has no placeholder `default.nix`.
Missing obligations remain gaps; the authorized legacy source
retirement does not establish replacement coverage.

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

Evidence: `nix build .#checks.x86_64-linux.integration-access.scenarios.lifecycle-local-bootstrap -L` builds a gate. The gate's `evidence` link is the run output: `result.json` (status, cases, durations, faults, receipts, error, traceback), `meta.json` (source revision, contracts, render profile, design reference and fixture hash), `browser/<spec>/` (`result.json`, `console.log`, screenshots, `trace.zip` and `failure.png` on failure, downloads, visual images), `journals/<node>/` (`crystal-forge*` and PostgreSQL journal, warnings, failed units). After failure, evaluate the registered scenarios' `.run.outPath` and inspect only outputs that already exist. Do not build `.run` to recover evidence: a crash or timeout could otherwise execute a second VM and replace the failed attempt. A package-build failure before VM execution produces no scenario execution evidence.

Isolation and shared store: each separately built scenario is its own NixOS test with fresh VMs and databases. Later cases in one scenario may use identities and rows created by earlier real workflows, carried through receipts. A task dependency never provides live state from another scenario. VMs see host store files through a read-only mount but register only their own closure. A build, publication, or copy proof MUST use an output first produced inside that scenario (per-run nonce) and MUST call `assert_store_absent` at the destination first.

Host resources (`sledge`, inspected 2026-10-17): 32 cores, 123 GiB RAM with about 45 GiB available. The controller is configured for 4 GiB and 4 cores. One VM run failed during server startup; peak usage and safe parallel capacity remain unmeasured. Run one VM test at a time until measurements establish capacity.

## Obligation map

### Verified corrective and initial Delivery code

Verified code revision: `bea0c29936c2220a8c1ba8dbfeb5750dacc18cde`.
[Hosted pipeline 2964502717](https://gitlab.com/crystal-forge/crystal-forge/-/pipelines/2964502717)
completed successfully. These are actual job results, not lint or intended
selection:

| Job | Result | Evidence |
| --- | --- | --- |
| `integration-access` | Passed, 23/23 cases | [Job 17294404506](https://gitlab.com/crystal-forge/crystal-forge/-/jobs/17294404506); archived result has the exact verified revision and unchanged case IDs. |
| `integration-delivery` | Passed, 8/8 cases | [Job 17294404507](https://gitlab.com/crystal-forge/crystal-forge/-/jobs/17294404507); exact source/attempt/drv/job/session/target receipts and cold/realized/completed/egress-denied markers. |
| `component-tests` | Passed | [Job 17294404508](https://gitlab.com/crystal-forge/crystal-forge/-/jobs/17294404508); ordinary components, module/runtime/tool and realized XCCDF/OpenSCAP checks. |
| `cve-processing-test` | Passed | [Job 17294404509](https://gitlab.com/crystal-forge/crystal-forge/-/jobs/17294404509). |
| `dashboard-visibility-tests` | Passed | [Job 17294404510](https://gitlab.com/crystal-forge/crystal-forge/-/jobs/17294404510). |
| `state-machine-tests` | Passed | [Job 17294404511](https://gitlab.com/crystal-forge/crystal-forge/-/jobs/17294404511); downloaded trace totals 515 passed, 0 failed, 0 ignored, 227 summaries, plus populated migration rehearsals. |
| `complexity-check` | Passed | [Job 17294404512](https://gitlab.com/crystal-forge/crystal-forge/-/jobs/17294404512). |
| `coverage-check` | Passed | [Job 17294404513](https://gitlab.com/crystal-forge/crystal-forge/-/jobs/17294404513). |

Both normal owner commands also exited 0 locally at that clean revision:
Access 23/23, driver 54.18 seconds, and Delivery 8/8, driver 79.32 seconds.
Local immutable evidence:

- Access: `/nix/store/798g3ypl22k5m3g7z2x65blwvndw37lc-vm-test-run-cf-it-access-lifecycle-local-bootstrap-run`.
- Delivery: `/nix/store/61x9c3ii0zw0b6c4pzln0af1vcm8zhfd-vm-test-run-cf-it-delivery-delivery-lifecycle-run`.

Reporting-only follow-ups do not change the executable cases. This successful
partial pipeline does not complete the four owners or resolve P3/TASK-492 and
TASK-496. TASK-495's separately authorized repair is in Review, not merged.

### Connected manual Delivery increment: rollback blocked

The existing ten-case increment was committed and pushed as
`3db47c8953e3cf1f29473be7595a660d69e6a988`. Its separate TASK-496 product repair
is `d11de05967b2ade5c7e1f833317039de134c307a` (original task commit `5c8f5ee8`).
The repair projects historical `boot_id` as null so `SystemState` decodes the
migrated row; it preserves `systems.boot_id` and conservative real-error
fallback. Stronger assertions are verification, not the production repair.

[Pipeline 2965867360](https://gitlab.com/crystal-forge/crystal-forge/-/pipelines/2965867360)
passed all eight jobs at that exact clean commit. Downloaded artifacts confirm
Access 23/23 and Delivery 10/10, exact source revision, signature rejection and
acceptance, two initial heartbeat IDs, and all eight controlled network receipts.
The database job passed 517 tests, 0 failed, 0 ignored across 229 summaries.
Component/package, CVE, dashboard, complexity and coverage jobs also passed.
That pipeline does **not** verify the later connected deployment increment.

The connected increment extends the same five VMs/database through two real
Git/native-evaluation/build/publication/UI-deployment workflows. Each target
output is physically absent and invalid before actual agent acquisition. A
pinned target-owned SSH identity reads the test service; no alternate
substituter or local-build fallback supplies the runtime output. The target
shares only immutable runtime prerequisites. Full cold-closure acquisition from
a warm store remains unproved (TASK-498).

CF's original builder signature is checked and cryptographically verified
before deliberate receiver-test signing. That proof is distinct from the
isolated receiver's enforced signature rejection/acceptance. The second job
also requires exact native attempt/derivation/output/claimant-session and signer
identity. Neither test-side signing nor a successful SSH login is CF signing
evidence by itself.

Both manual deployments actually activated. Independent target profile, running
path, revision marker, unchanged boot ID and unchanged agent key agree with
succeeded CF requests and durable history. Exactly one new full state appears
per transition, followed by two distinct heartbeats on that state. The browser
finds the operation, submits the real request and verifies the observed
generation and exact success event after reload. No `system_states` mutation,
fabricated report, stopped agent, synthetic completion or seeded authority is
used.

Latest executed local identities (`3db47c89-dirty`):

| Observation | Initial | First CF deployment | Second CF deployment |
| --- | --- | --- | --- |
| System | `f1addf9f-3f84-4b8b-9598-c244d29ae307` | Same registered system | Same registered system |
| Running output | `/nix/store/facrr7n1wbkhqs5i1xsxdwyga1hff8l0-nixos-system-targetA-test` | `/nix/store/wii9frwfwvkrl26a80kwizd3yrgq7q0l-nixos-system-targetA-test` | `/nix/store/cz8flym2r8cxn39k1xwfj6m6244cpdld-nixos-system-targetA-test` |
| Commit | Harness boot, not a CF-managed revision | `e5c115237b3d026f65dbfe74a86331e820a36c78` | `9d79b584629e73ccdee7806d04d2d46217759581` |
| Profile/generation | No system-profile link; observed generation null | `system-1-link`, generation 1, resolves to first output | `system-2-link`, generation 2, resolves to second output |
| State ID | `1` | `2` | `3` |
| Heartbeat IDs | `1`, `2` | `6`, `7` | `9`, `10` |
| Completed deployment | None | `64c90169-f0e3-40b9-abb6-741d1b8fe978` | `ca1a5d9f-f3ec-4c9b-a895-2afef5f8b82d` |

Boot ID stays `bc7d4cc1-0d18-4891-9848-507ceb28a5c7`; booted output remains
the independently observed initial output. No reboot claim is made.

The four added case names are:

- `manual-first-cf-deployment-activates-cold-output-and-reports-new-state`: passed.
- `second-authored-revision-builds-and-publishes-through-cf`: passed.
- `manual-second-deployment-reports-changed-state-and-correlated-heartbeats`: passed.
- `history-ui-rollback-restores-retained-output-and-correlates-current-state`: failed at mandatory eligibility.

The original ten case names and assertions remain. The final normal Delivery
gate fails: 13 passed, 1 failed. Fresh generation inventory after B reports A
as not current, `generation_snapshot_id=null`, `rollback_eligible=false`.
TASK-499 records the producer/consumer mismatch: native evaluation has no
available V1 artifact; normal Config inspection produces V2 evidence with an
isolated selector; deployment binding/retention/rollback require V1 authority.
No existing supported operation found in this investigation supplies that
authority. A production repair needs separate authorization. The rollback
request, restored generation/state and later rollback heartbeats have **not**
executed. The gate is not skipped or allowed to fail in CI.

Local results below are on `x86_64-linux`, with dirty-tree identity explicitly
recorded. `D` is
`/nix/store/mrs8rm4yvf5lk9vklqs01qvmzp1zl9n7-vm-test-run-cf-it-delivery-delivery-lifecycle-run`;
`A` is
`/nix/store/ci1m5937pfh4b1p3hzpmgxfa88vlk47z-vm-test-run-cf-it-access-lifecycle-local-bootstrap-run`.

| Command | Tested input | Platform | Exit | Executed/reused | Cases | Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| `sh packages/ci/integration-owner-build.sh integration-delivery` | `3db47c89-dirty`, connected code | x86_64-linux | 1 | New VM execution, driver 303.80 s | 13 passed / 1 failed | D; `integration-evidence/integration-delivery/` |
| `sh packages/ci/integration-owner-build.sh integration-access` | Same dirty source | x86_64-linux | 0 | New VM execution, driver 53.48 s | 23 passed | A; `integration-evidence/integration-access/` |
| `nix flake check --keep-going --no-update-lock-file -L --builders '' --max-jobs 1 --cores 2` | Same dirty source | x86_64-linux | 1 | Reused identical A/D run outputs; Delivery gate executed and failed | Same 23 and 13/1, not additional executions | A/D; TASK-499 failure |
| Normal hosted owner scripts | Clean `3db47c8953e3cf1f29473be7595a660d69e6a988`, earlier increment | x86_64-linux | 0 | Executed in pipeline 2965867360 | Access 23 / Delivery 10 passed | Jobs 17303956146 / 17303956147 |
| Documented offline state-machine workflow | Clean `3db47c89`, unchanged Rust repair inputs | x86_64-linux | 0 | Executed in hosted job, not rerun for fixture-only edits | 517 passed, 0 failed/ignored | Job 17303956151; downloaded raw trace |

The explicit database preparation phase and host-daemon isolation limitation
remain unchanged. Historical failures are preserved: TASK-497 configuration
type error, TASK-498 warm dependency signature rejection, and a corrected
direct-boot fixture GRUB attempt. The temporary Git receive process now runs
as its repository owner; no global ownership-check exception is used.

Review covered acceptance, backend/integrity, query/resource safety, UI/design
states, end-to-end observations and regression adequacy. The exact second-build
chain, one-state-per-transition, event identity and fresh rollback eligibility
checks were strengthened after review. TASK-500 records Started/success history
presentation and the rollback strategy-label difference from approved
`SystemDetail.jsx`; screenshots prove rendered workflow, not pixel/design
approval of those differences. No production UI changes were made. Broader
activation modes, BootOnly/reboot, isolation and recovery remain separate gaps.

### Delivery signature and unchanged-heartbeat continuation

The normal command `nix build .#checks.x86_64-linux.integration-delivery
--no-link -L --builders '' --max-jobs 1 --cores 2` exited 0 locally on
2026-10-20. All ten cases passed; driver time was 178.07 seconds. Source was
the working tree based on `d11de05967b2ade5c7e1f833317039de134c307a`, recorded
as dirty. This is not a clean-head or hosted result. Evidence:
`/nix/store/i6h35avw0z6kkyci7zxv5f2rlz9i8c2d-vm-test-run-cf-it-delivery-delivery-lifecycle-run`.

The publication fragment uses the exact CF-built input-addressed output and a
separate rooted receiving store inside services. The Unix-daemon client reports
`trusted=false`. All 727 dependencies were signed and imported through that
untrusted connection before the target test. The target remained absent and
invalid, then failed specifically because it lacked a signature by a trusted
key. Only source signature metadata changed before the same copy succeeded.
Receiver metadata and the authored nonce matched; NAR hash, size and references
were unchanged. No privileged import, disabled signature check, CA exemption,
receiver reconfiguration or publication race supplied success. This proves a
signature boundary independently of the existing trusted SSH publication
account; it does not prove that account enforces signatures.

The target assertion no longer accepts full-state fallback. Two real heartbeat
rows referenced the exact registered initial state S1; there was exactly one
full state and no comparison database-error log. A genuine observed state
transition S2 and subsequent heartbeat correlation remain open, so TASK-490.7
criterion 8 is not complete. The controlled IPv4/IPv6 network case also passed
again. Activation, rollback, Assurance and Upgrade remain incomplete.

The subsequent `nix flake check --keep-going --no-update-lock-file -L
--builders '' --max-jobs 1 --cores 2` exited 0 for `x86_64-linux`. Both registered
owners executed: Access 23/23 (50.90 seconds) and Delivery 10/10 (160.52 seconds).
Evidence:

- Access: `/nix/store/q9qhgv1qh35lj3s71wcg8s84q0985kwr-vm-test-run-cf-it-access-lifecycle-local-bootstrap-run`.
- Delivery: `/nix/store/y88mk7bplyb6hjxyqacsnsx8zhyf2dyn-vm-test-run-cf-it-delivery-delivery-lifecycle-run`.

The command omitted incompatible `aarch64-darwin`, `aarch64-linux` and
`x86_64-darwin` systems. It does not complete absent Assurance/Upgrade owners or
missing contract obligations. No new hosted result is claimed.

After explicit launcher/Cargo preparation, the documented offline isolated
state-machine workflow exited 0: 517 passed, 0 failed, 0 ignored across 229 Rust
summaries, including both TASK-496 regressions and populated migration cases.
The two new ignored-library selections raise that selection count from 284 to
286; fourteen integration targets retain 231 cases. The launcher created and
removed its temporary PostgreSQL cluster; no shared database was used. Offline
settings disable acquisition but do not prove host-daemon firewall isolation.

### Delivery controlled network continuation

The normal command `nix build .#checks.x86_64-linux.integration-delivery
--no-link -L --builders '' --max-jobs 1 --cores 2` exited 0 locally on
2026-10-19. The tested source was the working tree based on
`d11de05967b2ade5c7e1f833317039de134c307a`, recorded as dirty, not a clean
revision or hosted result. All nine cases passed; the driver took 77.08 seconds.
Evidence:
`/nix/store/lra2iw4nmplfr1lycslrc2dh4nfgg0l9-vm-test-run-cf-it-delivery-delivery-lifecycle-run`.

The new behavior-named network fragment uses the same five VMs and a second
test VLAN. The services VM hosts a controlled dual-stack HTTP endpoint. Each
controller, builder and target proves an `eth2` route, same-route reachability
from the narrowly permitted control UID, and product-actor rejection with an
increase in the exact IPv4/IPv6 firewall rejection counter. All eight network
receipts report a reachable control and boundary rejection. Required local
Git/API/SSH traffic remains usable; no external cache is probed. NAT isolation
remains enabled. This is a controlled VM boundary proof, not evidence that the
host Nix daemon has no network access.

The first executed continuation failed its IPv6 positive control because the
fixture blocked neighbor-discovery replies to global VLAN addresses. Narrow
neighbor-discovery permissions corrected the fixture; TCP rejection remained
enabled. Failed evidence remains at
`/nix/store/pmwnqnhijsa3yn0c1zi5p0camxv45jpy-vm-test-run-cf-it-delivery-delivery-lifecycle-run`.
Earlier two-minute and twenty-minute build invocations timed out before VM
execution and are not test passes.

The source includes the separately authorized TASK-496 product repair. The
observed periodic report is now a lightweight heartbeat. The existing assertion
still permits full-state fallback; exact real-agent unchanged/changed lineage
is not yet proved. Signature rejection, activation, rollback, Assurance and
Upgrade coverage remain open. No new hosted pipeline or full flake-check result
is claimed.

### Delivery source and authorized-build execution

The normal `sh packages/ci/integration-owner-build.sh integration-delivery`
aggregate passed eight mandatory cases in one five-role lifecycle. Initial
local evidence is from a working tree based on
`fc0c58361d00466fe50e3c46d6b37e01d4db7bf8`, not a clean-revision claim:
`/nix/store/bys8qx6hix4kvf1yqycjbiqq2pmr1sc4-vm-test-run-cf-it-delivery-delivery-lifecycle-run`.
Driver time was 100.17 seconds; configured memory totals 10 GiB across five
VMs (peak usage unmeasured). One controller/database and the same stores,
credentials and receipts serve the whole lifecycle. BuilderB starts after
BuilderA's completed job so this is not a competing-claim proof.

The proof authors a nonce-bearing Git commit, ingests that exact SHA, records
its native evaluation attempt and authoritative derivation, queries Nix output
metadata independently, and proves output absence on all five nodes before
releasing BuilderA. The real authenticated API builder materializes the
authorized derivation and realizes the output with `offline=true`. Its valid
output contains the independently authored nonce. Signed publication to the
test-owned SSH Nix store and exact job/session/output completion are mandatory.
BuilderB establishes a distinct session; the real target registers and reports
its actual running generation, including a subsequent periodic report.

NAT/public egress is blocked while local role traffic remains available.
Immutable inputs and baseline dependencies are prepared; the nonce output is
never built on the host. Runtime keys stay in the VMs. The destination trusts
generated public signing material in addition to its normal trust roots;
signature checking is not disabled. No response is mocked or result seeded.
The controller can publish input closures with its own runtime credentials.
Sharing immutable baseline preparation with the services store avoids copying
the full toolchain into its writable disk; the nonce output remains cold.

TASK-495 exposed and separately corrected an unsupported legacy Nix CLI flag.
Product commit `6844d185` was developed in its own task worktree and applied as
`fc0c5836` with owner authorization. TASK-496 remains an observed failing
heartbeat-classification variant: periodic reports are accepted but become full
state rows after a database comparison error. Positive reporting does not prove
the lightweight optimization. The prior package-build timeout launched no VM;
earlier failing test attempts remain immutable evidence, not passing runs.

Corrective hosted pipelines `2961382083` (`c2b77aad`) and `2961428492`
(`e07b4bd2`) passed all seven pre-Delivery required jobs: Access, component,
state-machine, CVE, dashboard, complexity and coverage. They do not establish
hosted results for the later Delivery/product-fix source. Final-head hosted
verification is recorded separately at handoff.

Hosted cold preparation exposed deep Nix recipe contexts in the generated
source fixture. Its direct derivation dependencies measured 2,525. Retaining
recipe paths without their transitive all-output contexts reduces that fixture
to five actual production-output dependencies. The source also uses the same
patched evaluator/Nix package pair as
the baseline; it no longer rebuilds a different evaluator during realization.
The unchanged eight-case normal aggregate passed after this correction in a
working tree based on `ee6e0ddb`, driver 62.67 seconds; evidence:
`/nix/store/fi1dd1iqfx1fm2k3lj26jcia2rkc1p62-vm-test-run-cf-it-delivery-delivery-lifecycle-run`.
The 2,525-to-five figure is scoped to this fixture, not a whole-pipeline recipe
or runtime comparison. No network fallback, output seeding or assertion
relaxation supplied missing inputs during verification.

Cold hosted preparation at `f369aacd` then exposed an invalid bootstrap object
(`strncpy`) while exporting the raw recipe closure, before any VM ran. Narrow
opaque recipe preparation had worked on the warm local store but was not a
complete cold-store prerequisite. The current native baseline retains the full
build closure once for all compatible roles, including those bootstrap source
objects. This is a deliberate cold-preparation cost, not a claim of globally
minimal recipes or 2,520 fewer whole-owner builds. The generated source remains
small; the separate selected-input metadata experiment is not retained. No
mandatory assertion or offline execution boundary was relaxed.

### Semantic source cleanup execution

At `590ee94c635154e027317414298ebe05e9b48bbd`, the normal
`sh packages/ci/integration-owner-build.sh integration-access` command exited
successfully after executing a fresh VM. All 23 existing case IDs, ordering,
receipt names and assertions passed. Driver time was 50.46 seconds; startup
remained 10.5 seconds once. Evidence:
`/nix/store/b21y37v9ffhlk74jr6ijslzvkfr33b2l-vm-test-run-cf-it-access-lifecycle-local-bootstrap-run`.
Hosted pipeline `2961337066` also passed Access, component, CVE and dashboard
jobs at that revision. Its state-machine job failed because root `initdb` is
forbidden; coverage/complexity were skipped, not passed. Follow-up pipeline
`2961382083` at `c2b77aad1e7f9d400ad1bb96fac2e5bbab6bf782` passed all seven
pre-Delivery required jobs, including root-runner PostgreSQL execution. The
earlier non-root local result alone did not establish that privilege boundary.

The canonical database path at `590ee94c` passed 515 tests and both populated
migration rehearsals with outbound connections denied in a loopback-only
network namespace. Outer and nested Nix settings disabled substitution,
remote builders and fallback; Cargo and SQLx ran offline. The host Nix daemon
was outside the namespace: this is not proof of a firewall-isolated daemon.
See [verification commands](../../agents/verification.md#consolidated-component-verification).
The current launcher additionally passed all 515 selected tests after the root
privilege and short-socket changes (working tree based on `c2b77aad`, not a
clean-revision claim), exit receipt 0, 18m14s; log
`/tmp/opencode/task490-offline-root-launcher.log`. An earlier long-TMPDIR
attempt failed at socket initialization before tests; its evidence is retained.

Scenario paths are relative to `checks/`. Authoritative observations supplement `result.json`; the case-local authority rules above apply. Each separately built scenario runs in a fresh VM/database; a prerequisite scenario's runtime output is never reused. A task dependency supplies code/immutable fixture preparation only, not another check's live data. Within one scenario, later cases may use identities and records created by earlier real workflows; carry generated identities in the existing receipt mechanism. Every scenario must create its own real starting evidence through setup operations and must not seed the result under test. The coordinator owns composition and registration; tracking boundaries alone do not define VM boundaries.

### Behavior-to-scenario composition and VM boundaries

Stable contract IDs and scenario names define functional ownership. Backlog task IDs identify tracking and accepted selections; they do not own behaviors, source paths or VMs. The table records current and intended behavior composition. Bootstrap, selected sessions, selected admin-denial authorization and seven Admin-form cases are implemented and have recorded passing evidence at `c254b1c5`; the post-cleanup rerun is pending. The coordinator owns each scenario's `.nix` composition and explicit owner registration. Behavior-named fragments have disjoint paths under the owner's `scenarios/fragments/` directory; they do not each define or register a NixOS test derivation.

| Scenario | Behavior contracts and cases to compose | Why this VM boundary is retained | Tracking |
| --- | --- | --- | --- |
| `integration-access/scenarios/lifecycle-local-bootstrap` | `access.lifecycle`, `access.configuration`, `access.shell`: bootstrap, local-mode session/CSRF/sign-out, authorization, closed registration, admin/environment forms, shell/navigation and per-user preferences. Each case uses identities and records produced earlier in this same real workflow; generated identities go through receipts. | One local-auth controller configuration and one evolving database support these cases. Preserve the dependency order below. | TASK-490.1/.2/.3/.4 |
| `integration-access/scenarios/configuration-notifications` | `access.configuration`: real event, in-app preference/delivery and email delivery cases. | Separate from local bootstrap because proving email requires a different machine configuration: notification email enabled with a local receiver, plus a real delivery/build event. Recreate that event here; another scenario's VM or output is not reusable. | TASK-490.5; services prerequisite .7; event preparation .11 |
| `integration-access/scenarios/lifecycle-oidc` | `access.lifecycle`: authorization-code, callback, role mapping and sign-out cases. | Separate because `auth_mode = "oidc"` and the local OIDC provider/redirect setup are incompatible with the local-mode bootstrap proof. | TASK-490.6; services prerequisite .7 |
| `integration-delivery/scenarios/delivery-lifecycle` | `delivery.evaluation`, `delivery.build`, `delivery.publication`, `delivery.activation`: real ingestion/evaluation, build scheduling/transports, publication variants, activation, rollback and recovery. Use generated commit/build/cache/target identities through receipts. | One controller/database, service fixture, two independent API-only builders and a target agent support the ordered workflows. Keep outputs and state inside this run. Separate builders provide distinct store/session authority; no tracking-specific VM is justified. | TASK-490.7 foundation; .8/.9/.10/.11/.12/.13/.14/.15 |
| `integration-assurance/scenarios/evidence-remediation-lifecycle` | `assurance.scanning`, `assurance.policy`, `assurance.remediation`, `assurance.interchange`: scanning, target, policy and import cases; then triage, waiver, acceptance, POA&M and verification/closure/reopen; then exports of records created earlier in the same scenario. | One controller/target/service configuration and a single actual finding/evidence chain can support these ordered cases. Keep one fresh VM/database for the whole scenario; use receipts for finding, acceptance, plan and target identities. Do not create tracking-specific derivations or depend on another scenario's outputs. Reconsider only if a proven incompatible machine configuration or isolation invariant appears, and record that concrete reason here before splitting. | TASK-490.16/.17/.18/.19/.20/.21/.27/.28/.29/.22 |
| `integration-upgrade/scenarios/version-transition` | `upgrade.continuity`: real old-version setup workflow, current-version migration, continuity and recovery. | Separate because it intentionally changes the running server version and migrates an in-place database; that compatibility/migration state must not contaminate current-version Delivery or Assurance evidence. Old and current phases remain sequential inside its own fresh VM/database. | TASK-490.23 |

The implemented local Access lifecycle directly concatenates bootstrap → `sessions.py` → `authorization.py` → `admin-forms.py` into one Python namespace on one controller and one evolving database. Bootstrap establishes `controller`, Admin helpers and the `admin.userId` receipt before sessions creates additional sessions. Authorization runs after the fresh-install assertions, creates the Operator/Viewer/target actors and exports `authz_state()` for the forms' protected-state cancel checks. Its Admin positive controls restore the target, retry policy and classification configuration to the installed values, including the disabled banner. Forms depend on those restored values and on design user `u3` still being absent. The corresponding `.cjs` files provide browser phases in this same scenario; each form phase returns to Python for authoritative SQL observations before the next mutation. These are explicit state dependencies, not independently runnable fragment scenarios.

The scenario boundaries are the local Access lifecycle, notification delivery, OIDC lifecycle, Delivery lifecycle, Assurance evidence/remediation lifecycle, and Upgrade version transition. Local Access, Delivery and Assurance are registered. Other scenarios remain planned. Their boundaries reflect incompatible auth/service configuration, separate owner contracts, or deliberate in-place version/database transition—not one VM per tracking task. No independent scenario may use another scenario's rows, sessions, receipts, generations, or output.

The local-bootstrap, Delivery and Assurance lifecycle attributes exist. Other targets
below remain planned and must be explicitly registered before execution. Each
focused gate has its normal owner aggregate after the semicolon.

| Scenario | Focused command; owner aggregate |
| --- | --- |
| `integration-access/scenarios/lifecycle-local-bootstrap` | `nix build .#checks.x86_64-linux.integration-access.scenarios.lifecycle-local-bootstrap -L`; `nix build .#checks.x86_64-linux.integration-access -L` |
| `integration-access/scenarios/configuration-notifications` | `nix build .#checks.x86_64-linux.integration-access.scenarios.configuration-notifications -L`; `nix build .#checks.x86_64-linux.integration-access -L` |
| `integration-access/scenarios/lifecycle-oidc` | `nix build .#checks.x86_64-linux.integration-access.scenarios.lifecycle-oidc -L`; `nix build .#checks.x86_64-linux.integration-access -L` |
| `integration-delivery/scenarios/delivery-lifecycle` | `nix build .#checks.x86_64-linux.integration-delivery.scenarios.delivery-lifecycle -L`; `nix build .#checks.x86_64-linux.integration-delivery -L` |
| `integration-assurance/scenarios/evidence-remediation-lifecycle` | `nix build .#checks.x86_64-linux.integration-assurance.scenarios.evidence-remediation-lifecycle -L`; `nix build .#checks.x86_64-linux.integration-assurance -L` |
| `integration-upgrade/scenarios/version-transition` | `nix build .#checks.x86_64-linux.integration-upgrade.scenarios.version-transition -L`; `nix build .#checks.x86_64-linux.integration-upgrade -L` |

### Inventory reading key and inspected sources

Research baseline: `2c34186ed597aa064506384907a06e943b0d227d`, inspected
2026-10-18 in the existing TASK-490.1 worktree under active TASK-490.24.
Concurrent lower-level consolidation is in progress. Its final file/command
references and execution results are not assumed here. The source index names
inspected boundaries and relevant mechanism locations; it is not a claim that
every implementation line or every guard has been independently executed.

The twelve contract-family tables below replace the old suite-shaped obligation list. Route families are discovery scope, not route-count coverage. Brace lists enumerate suffixes; `:id` denotes the registered parameter. Methods apply only as listed. All `/api/v1` prefixes are omitted in the route inventory unless another prefix is shown. Every registered product family in `bin/server.rs` is assigned below, including aliases, conditional authentication, streams, and UI fallback. No generated registry or additional database is required.

Source shorthand is relative to the repository root:

| Reference | Inspected source and purpose |
| --- | --- |
| S-router | `packages/default/crates/cf-server/src/bin/server.rs`, route construction and conditional auth/UI fallback; startup/migration/recovery ordering. |
| S-api | `packages/default/crates/cf-server/src/handlers/api/`; the named `.rs` file in a row is its handler. `rbac.rs`, `auth_session.rs`, and `auth/extractors.rs` identify session/role/CSRF mechanisms. There is no global router auth/CSRF layer that proves every handler's boundary. |
| S-worker | `packages/default/crates/cf-server/src/{services,queries}/`; named modules locate server-owned policy, transaction, identity and evidence mechanisms. These are observation locations, not independent expected values. |
| S-wire | `packages/default/crates/cf-protocol/src/{agent,builder,cache,source_artifact}.rs`; supported payload/strategy/capability variants. |
| S-agent | `packages/default/crates/cf-agent/src/{bin/agent,system_state,network,deployment/agent}.rs`; state gathering, heartbeat, cache copy and activation. Agent authentication is in server `handlers/agent_request.rs` and `handlers/agent/*.rs`. |
| S-builder | `packages/default/crates/cf-builder/src/{builder/api_client,builder/mod,builder/cve_scanner,build,cache}.rs`; server `handlers/builder_request.rs`, `handlers/api/builders.rs`, `queries/{builders,build_jobs,build_reservations,cve_scan_leases}.rs`. |
| S-config | `modules/nixos/crystal-forge/default.nix`, `packages/default/crates/cf-config/src/config/*.rs`, `cf-config/src/{cache_credentials,ca_bundle}.rs`; generated configuration, service roles, runtime secrets and TLS trust. |
| S-ui | `packages/web-ui/src/routes.rs`, `views/*.rs`, and `components/`; actual route/query state, forms, drawers and navigation. Specific consumers are named below. |
| A-contract | [Adopted maintenance contract](integration-test-contract.md), Sections 1–4: supported boundary ownership, real-component proof and independently authored facts. Active TASK-490 child acceptance criteria refine the named cases; a new discovery does not silently extend a child task. |
| A-systems | `docs/design/CrystalForge/docs/crystal-forge-systems-design/{systems-view-design-v0.1,system-cves-chunk-1}.md`: owner decisions, target intent, non-disclosure, read-only reads and UI authority. Only accepted clauses control; AS-BUILT/PROPOSED sections do not. |
| A-continuity | `docs/design/CrystalForge/cve-poam-evidence-continuity-design-spec.md`, normative product contract, Sections 2, 6–24, 27–29 and 32. It supersedes SC1's older Current-action retention gate where stated. |
| A-design | `docs/design/CrystalForge/components/`: approved component states referenced below. Mock names and counts may supply authored fixture values, never backend policy or evidence authority. Missing states use D3/D6. |
| E-domain | `docs/knowledge/builders/verified-source-evaluator-contract.md`, `deployment/manual-deployment-queue-contract.md`, `evaluation/evaluation-snapshot-retention-and-rollback.md`, and `ui/system-cves-sc1-contract.md`: inspected mechanism/precedence pointers. Their `implemented` metadata does not approve new requirements. |

Scenario abbreviations use the six composition targets: **L** = Access
`lifecycle-local-bootstrap`; **N** = Access `configuration-notifications`;
**O** = Access `lifecycle-oidc`; **D** = Delivery `delivery-lifecycle`;
**A** = Assurance `evidence-remediation-lifecycle`; **U** = Upgrade
`version-transition`. L and D are registered. Behavior IDs and owner/scenario
names define primary execution ownership. **Tracking / status** keeps task
selection separate; `.n` abbreviates `TASK-490.n`. `unassigned` details require
coordinator selection before implementation. No task or row creates a VM.

For every mutation case, use a valid authorized positive control and a separately authenticated disallowed actor, then independently compare protected records and external effects. Test exact ownership, hidden/absent identity, invalid/stale input, retry, cancellation and reload where those conditions apply. Do not conflate session last-seen updates with protected domain changes. A read's HTTP success or a UI toast is not authoritative persistence evidence. Expected commits, option values, findings, policy inputs, target paths and export records come from authored fixtures or independent tools, never CF's response under test.

### Registered HTTP surface families

This table is a complete family-level discovery index for S-router, not a per-endpoint pass matrix. Contract rows below define the proof. Method groups on one row remain distinct operations for implementation; proving one does not cover the others.

| Family / methods and registered patterns | Handler / gate observed in source | Contract family |
| --- | --- | --- |
| `GET /status`; `POST /system_state`, `/agent/{heartbeat,state,deployment-started,deployment-failed}` | `status.rs`, `handlers/agent/*.rs`; agent body signature and registered hostname/key. `/system_state` aliases state ingestion. | `delivery.activation`, installation portion of `access.lifecycle` |
| `POST /webhook` | `handlers/webhook.rs`; payload parsing and asynchronous commit insertion, no signature guard in handler (D4). | `delivery.evaluation` |
| `GET /api/auth/{whoami,setup-status}`; `POST /api/auth/logout`; local `POST /api/auth/local/{login,register}`; OIDC `GET /api/auth/oidc/{login,callback}`; dev `POST /api/auth/dev/login` | `auth_*.rs`; public context/bootstrap, cookie sessions, logout CSRF; mode-conditional routes. Dev mode is release-rejected. | `access.lifecycle` |
| `GET /admin/{users,oidc-mappings,audit-events,server-info,classification-config,automatic-retry-policy,setup-progress,config-health}`; `POST /admin/{users,oidc-mappings,setup-wizard/dismiss,setup-wizard/agent-acknowledge}`; `PATCH,DELETE /admin/users/:id`; `DELETE /admin/oidc-mappings/:id`; `PUT /admin/{classification-config,automatic-retry-policy}` | `admin.rs`, `setup_wizard.rs`, `config_health.rs`; Admin gates except intentionally public classification read. Do not infer uniform CSRF. | `access.configuration` |
| `GET,PATCH /user/{preferences,notification-preferences}`; `POST /user/preferences/initialize`; `GET /user/notifications`; `POST /user/notifications/{read-all,dismiss-all,:notification_id/read}`; `DELETE /user/notifications/:notification_id`; `GET /user/sessions`; `POST /user/sessions/revoke-all`; `DELETE /user/sessions/:session_id` | `user_*.rs`; per-session user identity, user-owned records; session mutations explicitly validate CSRF. | `access.lifecycle`, `access.configuration` |
| `GET /navigation/badges`; `POST /navigation/acknowledge`; `GET /dashboard/{summary,activity}` and `/nixos/options` | `navigation.rs`, `dashboard.rs`, `nixos_options.rs`; inspect each read's scope, metadata availability and actor. | `access.shell`; domain values by Delivery/Assurance |
| `GET,POST /environments`; `GET,PATCH,DELETE /environments/:id`; `GET /environments/policies-map`, `/policies`; `GET,PATCH /environments/:id/policies`; `GET /environments/:id/{caches,compliance-assignments}` | `environments.rs`, `caches.rs`, `compliance.rs`; authenticated membership-filtered reads, Admin resource mutations; policy/cache bindings retain their domain owners. | `access.configuration`, `assurance.policy`, `delivery.publication` |
| `GET,POST /systems`; `GET,PATCH /systems/:id`; `PUT /systems/:id/{evaluation-lifecycle,public-key}`; `POST /systems/:id/{sync,deactivate,deploy,rollback,rollback-generation,verify-generation-closure}` | `systems.rs`, `systems/evaluation_lifecycle.rs`; role and environment checks per action, explicit CSRF on selected mutation handlers. | `access.configuration`, `delivery.activation` |
| `GET /systems/:id/{commits,generations,deployment-status,history,agent-events,evaluated-options,evaluation-summary,evaluation-module-sources}`; `POST /systems/:id/{evaluations,config-inspections,config-observations}/:revision`; `GET /systems/:id/config-observation-requests/:request_id`, `/systems/:id/config-observations/by-id/:observation_id` | `systems.rs`; scoped retained identity and read paths; whole-commit evaluation queue action Admin-only plus CSRF; Config actions have separate authorization. | `delivery.evaluation`, `delivery.activation` |
| `GET,POST /flakes`; `POST /flakes/sync`; `PATCH,DELETE /flakes/:id`; `POST /flakes/:id/{sync,refresh,accept-rewrite}`; `GET /flakes/timelines`, `/flakes/:id/revisions/:revision/outputs`, `/flakes/:id/revisions/:revision/modules/:module/declarations`, `/flakes/:id/commits/:hash/diff`; `GET,PUT,PATCH,DELETE /flakes/:id/credentials`; `POST /flakes/:id/credentials/test` | `flakes.rs`; RequireAuth/Operator/Admin and scoped reads, encrypted credential management. | `access.configuration`, `delivery.evaluation` |
| `GET /commits/{eval-queue,eval-history}`; `POST /commits/eval-queue/reorder`; `GET /commits/:commit_id/eval/{stream,logs,policy-matrix,dependency-graph}`, `/commits/:commit_id/eval/attempts/:attempt_id/policy-matrix`; `POST /commits/:commit_id/{re-evaluate,cancel-evaluation,force-cancel-evaluation}` | `commits.rs`, `commits/visibility.rs`, `commits/log_registry.rs`; scoped read visibility, role gates on queue actions; re-evaluate has Admin and CSRF checks. | `delivery.evaluation` |
| `GET,POST /builders`; `POST /builders/resolve-id`; `GET,PATCH,DELETE /builders/:id`; `DELETE /builders/:id/permanent`; `PUT /builders/:id/public-key`; `POST /builders/:id/regenerate-keypair`; `PATCH /builders/:id/environments`; `GET /builders/:id/metrics` | `builders.rs`; Admin management versus separately signed resolve/session bootstrap. | `access.configuration`, `delivery.build` |
| `POST /builders/:id/{session,heartbeat}`; `GET,POST /builders/:id/next-job`; `POST /builders/:id/jobs/:job_id/{start,progress,publish-derivation-closure,complete,fail,logs,finalize-cancelled}`; `GET /builders/:id/jobs/:job_id/{derivation-archive,derivation-manifest,source-archive,status}`; also `POST .../derivation-archive` for delta | `builders.rs`, `handlers/builder_request.rs`; signed method/path/timestamp/body, builder identity/session/job ownership and manifest authorization. | `delivery.build`, publication binding in `delivery.publication` |
| `GET /build-jobs`, `/build-jobs/{recent,:id}`, `/build-jobs/:job_id/logs/stream`; `POST /build-jobs/:id/{cancel,requeue,force-cancel,prioritize,move-up,move-down}` and `/build-queue/reorder` | `builders.rs`; Admin cancel/force-cancel, Operator/Admin ordering, scoped retry and authenticated log access. | `delivery.build` |
| `GET,POST /caches`; `GET,PUT,DELETE /caches/:id`; `GET,PUT /caches/:id/environments`; `GET /caches/:id/metrics`; `POST /caches/test-credentials`, `/caches/:id/test-credentials`, `/caches/niks3/discover`, `/caches/:id/niks3/discover`, alias `/api/caches/niks3/discover` | `caches.rs` and `caches/*.rs`; Admin mutation/probe gates, sanitized reads and in-memory secret merge. Probe's supplied CSRF state must match; no assumption of generic mutation semantics. | `access.configuration`, `delivery.publication` |
| `GET /cache-push-jobs`, `/cache-push-jobs/:id`; `POST /cache-push-jobs/:id/{retry,cancel}`, `/cache-push-jobs/{bulk-retry,bulk-cancel}` | `caches.rs`; queue controls and authorization, per-destination publication state. | `delivery.publication` |
| `GET /cves`, `/cves/{grouped,inventory/groups,inventory/members,inventory/pairs,stats,packages,export,summary,vulnerabilities,top-systems,scan-freshness}`; `GET /cves/:cve_id{,/systems,/fleet,/justifications}`; `POST /cves/{rescan-fleet,rescan/:derivation_id,batch-detail,batch-triage}`, `/cves/:cve_id/triage`; `POST,DELETE /cves/:cve_id/justification` | `cves.rs`, `dashboard.rs`, `poam.rs`; fleet authorization is separate from system-target authorization; triage actor scope and CSRF. | `assurance.scanning`, `assurance.remediation`, `assurance.interchange` |
| `GET /systems/:id/{cves,cve-inventory,cve-inventory-page,cve-inventory-sources,cve-scan-eligibility}`; `PUT /systems/:id/cves/:cve_id/justification`; `GET,POST /systems/:id/cves/:cve_id/triage`; `POST /systems/:id/cve-scan`, `/flakes/:id/configs/:config_name/cve-scan`; `GET /cve-scans/:id` | `systems.rs`, `flakes.rs`, `poam.rs`; authorize system/target before source disclosure, scoped mutation and Current evidence authority. | `assurance.scanning`, `assurance.remediation` |
| `GET /scanning/{stats,queue,scans,scans/:scan_id,systems,systems/:system_id/scans,activity,schedule,deployed}`; `PATCH /scanning/scans`; `PUT /scanning/schedule`; `POST /builders/:id/cve-scans/{claim,heartbeat,complete,fail}` | `scanning.rs` Admin console; `builders.rs` signed lease-bound scanner operations with payload limits. | `assurance.scanning` |
| `GET /hardening/{summary,top-services,systems}`; `GET /systems/:id/{hardening,hardening-inventory,hardening/justifications,hardening-scan-eligibility}`; `PUT /systems/:id/hardening/:service_name/justification`; `POST /systems/:id/hardening-scan`; `GET /hardening-scans/:id` | `hardening.rs`; Admin fleet summaries, scoped system/target reads and justified writes. | `assurance.scanning` |
| `GET,POST /deployment-policies`; `GET,PUT,DELETE /deployment-policies/:id`; `GET /deployment-policies/:id/deletion-eligibility`; `POST /deployment-policies/bulk-delete`; `POST /deployments/commit/:commit_id/approve`; `GET /deployments/commit/:commit_id/{approvals,rollout}/:policy_id` | `deployment_policies.rs`, `deployments.rs`; Auth read, Operator edit/create, Admin deletion, configured approval role. | `assurance.policy`; rollout execution `delivery.activation` |
| `GET,POST /compliance/grouping-schemes`; `PUT,DELETE /compliance/grouping-schemes/:id`; `GET,POST /compliance/bundles`; `GET,PUT,DELETE /compliance/bundles/:id`; `GET /compliance/bundles/:id/{deletion-eligibility,systems}`, `/compliance/bundles/:id/systems/:system_id/evidence`, `/systems/:system_id/compliance` | `compliance.rs`; authenticated catalog, Admin management, scoped system evidence reads. | `assurance.policy` |
| `POST /policy-versions/:version_id/{trust,publish}`, `/policies/:policy_id/drafts`, `/compliance/bundle-versions/:version_id/{trust,publish}`, `/compliance/bundles/:bundle_id/drafts`; `GET /policy-versions/:version_id/usage`, `/compliance/bundle-versions/:version_id/{policies,requirements}` | `compliance.rs`; distinct trust/publication/version operations; Admin mutation gates and explicit CSRF on these workflows. | `assurance.policy` |
| `POST /compliance/assignments`, `/compliance/assignments/preview`; `GET,PUT,DELETE /compliance/assignments/:id`; `GET /compliance/assignments/:id/effective-policies`, `/systems/:id/{compliance-assignments,effective-policies}` | `compliance.rs`; Admin assignment mutation and scoped effective-policy/evidence reads. | `assurance.policy` |
| `GET /compliance/frameworks`, `/compliance/frameworks/:id/{versions,mapped-policy-versions}`, `/compliance/framework-versions/:fv_id/requirements`, `/compliance/requirement-versions/:rv_id/children`, `/compliance/bundle-versions/:bv_id/requirement-coverage`; `POST /compliance/framework-versions/:fv_id/recover`; `GET,POST /policy-versions/:pv_id/requirement-mappings`; `PUT,DELETE /policy-versions/:pv_id/requirement-mappings/:m_id` | `framework_requirements.rs`; authenticated catalog, Operator/Admin mapping edits with CSRF, Admin recovery. | `assurance.policy` |
| `POST /compliance/xccdf/{preview,import}`; `GET /compliance/bundle-versions/:version_id/xccdf`, `/compliance/assignments/:assignment_id/xccdf`, `/policy-versions/:version_id/export`; `POST /policies/interchange/{export,preview,import}` | `compliance.rs`; role gates, body limits, receiver validation and transactional reconciliation. | `assurance.interchange` |
| `GET,POST /poams`; `POST /poams/cves`; `GET /poams/{dashboard,dashboard/watchlist,rollups/systems,rollups/bundles,relationships/findings,relationships/assignments,relationships/cves,compatible,assignees}`; `GET,PATCH /poams/:id`; `GET /poams/:id/compatible`; `POST /poams/:id/{transition,notes,milestones,findings,cve-findings,assignments,verify,close,reopen}`; `PATCH,DELETE /poams/:id/milestones/:milestone_id`; `DELETE /poams/:id/{findings/:finding_id,cve-findings/:finding_id,assignments/:assignment_version_id}` | `poam.rs`, `services/poam.rs`; RequireAuth, actor role/environment scope, CSRF, domain locking/version/precondition checks. | `assurance.remediation` |
| `GET,POST /finding-waivers`; `GET /finding-waivers/:id`; `POST /finding-waivers/:id/status`; `GET /acceptances`; `POST /acceptances/:source/:id/{renew,convert}`; `GET /{poams,acceptances,register}/export` | `poam.rs`, `acceptance_register.rs`, `register_export.rs`; source-owned scope/version and transactional actions; independently scoped exports. | `assurance.remediation`, `assurance.interchange` |
| Embedded UI `GET` fallback/static assets; core-server unmatched 404; browser route catch-all | `handlers/ui.rs`, S-router feature guard, S-ui routes/shell. | `access.shell`; core packaging lower-level mechanism |

### 1. `access.lifecycle` — installation, identity and authentication

Authority: A-contract, selected TASK-490.2 cases, A-design Admin role catalogue. Sources: S-router, S-api `auth_*.rs`, `user_sessions.rs`, `rbac.rs`, `auth/extractors.rs`, S-config. Owner/scenario: **Access/L**, except OIDC **Access/O**.

| Obligation | Actor / initial state → action | Independent expected result | Authoritative observation | Tracking / status |
| --- | --- | --- | --- | --- |
| `local-bootstrap` | Fresh local-auth module/database → start, register first Admin, sign in/reload, wrong-password attempt | Config selects local; source migration set applied; app-role clients use Unix socket; only Admin role; durable session; wrong password 401 without session/cookie. | Generated config, migrations table, filtered `pg_stat_activity`, users/roles/sessions, browser reload. | .1; **Integration-covered**, five cases, latest recorded sixteen-case rerun `0105e512`. |
| `sessions` | User with retained, selected and separate browser sessions → missing/mismatched CSRF DELETE, valid revoke, sign out | Invalid CSRF 403 changes no lifecycle rows; valid revoke 204 invalidates selected session only; revoked GET 401, retained GET 200; browser logout 204, reload/protected navigation at `/login`. | Exact `user_sessions`, HTTP responses, browser result/receipts. | .2; **Integration-covered**, five cases, `0105e512`; not revoke-all/expiry coverage. |
| `authorization` | Real Operator/Viewer plus Admin positive controls → user POST/PATCH, retry/banner PUT, Viewer `/admin` | Selected ten denied requests return 403 `forbidden`, no protected mutation; Viewer has no usable Admin UI mutation. | Users/roles/memberships/admin-audit/settings before/after; independent Viewer browser. | .2; **Integration-covered**, six cases, `0105e512`; details in execution record. |
| `session-remainder` | Authenticated user plus foreign/expired/revoked sessions → list, revoke-all, attempt foreign revoke and protected reads | Own-session isolation and accepted expiry/disabled-account rules; no foreign invalidation; logout cannot leave a usable revoked credential. Exact cases beyond selected proof need authored expectations. | Sessions/users and independent cookie jars, reload and protected API. | .2 extension **unassigned**; `planned`. |
| `environment-scope` | Nonmember Operator/Viewer and Admin control → selected cross-environment request | Membership/hidden-resource expectation must be selected independently. | Scoped HTTP plus protected rows and side-effect observers. | .2; `blocked: D1-scope`. |
| `registration-closed` | Existing local user, registration disabled → second registration | Approved exception/status and no unauthorized new user/session. | Users/roles/sessions before/after. | .2; `blocked: D1-registration`. |
| `oidc` | Local provider with authored groups and roles → authorization-code redirect/callback, denied callback, logout | Genuine provider exchange issues only intended CF identity, roles and memberships; failed state/nonce/code validation creates no authenticated session; logout invalidates CF session. | Provider exchange plus CF users/mappings/sessions, browser redirect chain. | .6, O; `planned`, needs .7 services. |

### 2. `access.configuration` — forms, settings and notification lifecycle

Authority: A-contract, TASK-490.3/.4/.5, A-design `AdminView.jsx`, `EnvironmentsView.jsx`, `ProfileView.jsx`, resource form references. Sources: S-api `admin.rs`, `environments.rs`, `setup_wizard.rs`, `config_health.rs`, `user_*.rs`; S-ui views and form components. Owner/scenario: **Access/L** unless N is stated.

| Obligation | Actor / initial state → action | Independent expected result | Authoritative observation | Tracking / status |
| --- | --- | --- | --- | --- |
| `admin-users` | Admin with fresh list → create user, edit role/environment, cancel, disable, login as disabled user | Exact authored identity/assignments persist through reload; cancel changes nothing; disabled user cannot sign in (401). Denials beyond selected .2 proof are separate. | `users`, `user_role_assignments`, memberships and audit rows; second login. | .3; **integration-covered subset**: UI create, role edit, create/edit cancel, disable and failed login at `c254b1c5`. Explicit nonempty environment assignment and audit identity remain `planned`. |
| `admin-settings` | Admin → save classification and automatic retry policy; inspect runtime/config health and audit | Authored settings survive read/reload; retry values configure future decisions rather than retroactively rewriting evidence; audit identifies real actor. Runtime diagnostics expose no secrets. | Settings/audit DB and generated runtime config; browser/server responses. | .3; **integration-covered subset**: retry-policy save, classification enable/marking save and reload at `c254b1c5`. Future retry effects, diagnostics and audit remain `planned` (some tracking selections **unassigned**). |
| `resources` | Admin, initially empty environments → create/edit/duplicate/cancel/delete; try deleting used environment | Authored metadata survives reload; duplicate creates no extra row; cancel preserves rows; used-resource deletion preserves references. | Environments, dependent systems/bindings and reload. | .3 environment cases; used-resource guard **unassigned**; `planned`. |
| `resource-form-bindings` | Authorized user → flake, system, builder and cache add/edit/deactivate/delete/key/environment forms | Each distinct resource binds its own identity/permission; save persists only authored fields, cancel changes none, credentials remain redacted. Provider/build/deploy effects are proved by Delivery. | Respective resource, credential and assignment tables; independent reload. | **Unassigned**: .3 explicitly excludes these forms; intended L extensions; `planned`. |
| `preferences` | Two users → initialize/patch profile, theme/layout and notification choices; reload each | Preferences are per-user, durable and do not overwrite the other user's values. | `user_preferences`, notification-preference rows and independent browser contexts. | .4; `planned`. |
| `notifications` | Real event plus two users/channels → receive, read one/all, dismiss one/all; email enabled/disabled/retry | Only eligible recipients receive event; lifecycle mutations affect own records; disabled email sends none; enabled email reaches receiver without changing domain event identity. | Notification/outbox/preference DB, receiver requests and browser reload. | .5, N; retry/recipient variants need explicit tracking detail; `planned`, .7/event prerequisite. |
| `setup-and-oidc-admin` | Admin after bootstrap → setup progress/dismiss/agent acknowledgement and mapping CRUD | Progress reflects actual configured prerequisites; dismissal/acknowledgement persist per accepted scope; mappings alter only intended future identity assignments. | Setup/mapping/audit records, subsequent OIDC login in O. | .6 mapping execution; setup UI/API **unassigned**; `planned`. |
| `failed-save-states` | Admin with unsaved draft → bounded real server outage or pending request | Exact designed draft/empty/pending/disabled-save expectation required; DB remains unchanged for a failed write. | DB before/after, browser draft, restored service and reload. | .3; `blocked: D3-form-state` only for unspecified presentation/retention states. |

### 3. `access.shell` — routing, shared mechanics and module boundary

Authority: A-contract and A-design `Shell.jsx`, `DashboardView.jsx`, shared design states. Sources: S-ui `routes.rs`, `components/layout/{app_shell,sidebar,topbar}.rs`, `components/dialog_focus.rs`, onboarding, S-config/S-router. Owner/scenario: **Access/L**; domain values retain their domain owner.

| Obligation | Actor / initial state → action | Independent expected result | Authoritative observation | Tracking / status |
| --- | --- | --- | --- | --- |
| `navigation` | Signed-in and anonymous contexts → sidebar/user menu, direct URLs, reload, back/forward, unknown route | Approved destinations/query identities survive navigation; unauthenticated protected navigation reaches login; unknown route shows designed NotFound; role-limited entries remain limited. | Browser URL/history, session identity and rendered destination; no route-count inference. | .4; `planned`. |
| `shared-interaction` | Designed drawer/modal/menu and nested caller → keyboard open/close/Escape, focus traversal/restore, narrow viewport | Approved containment, close/cancel and focus return hold; caller retains target and return destination. Test shared mechanics once; distinct caller bindings in D/A. | Browser focus, URL and no-write DB observation for cancel. | .4; domain nesting cases **unassigned**; `planned`. |
| `dashboard-badges` | Two scoped users, real events → dashboard/badges, acknowledge, reload | Authored event scope determines visible badges/activity; acknowledgement changes only intended user's attention state. Domain totals must agree with independently known workflows. | Attention/navigation DB; D/A event records; browser. | .4 shell; domain values .8/.11/.16/.21; extra bindings **unassigned**; `planned`. |
| `module-runtime` | Production module server/client/API-builder selections, file secrets, explicit config → start services | Role-specific config/units and runtime credential permissions match selected options; API-only builder has no DB dependency; secrets not emitted in UI/logs. | Generated TOML/env summaries, units/permissions, DB clients and startup; ordinary module/component assertions. | .7; **integration-covered subset**: real server/client/API-builder startup, no builder DB section and no remote controller DB clients in D. Full configuration/security matrix remains `planned`. P3/TASK-492 maintenance startup and TASK-496 heartbeat-classification variants remain **failing**, not repaired by positive role proof. |
| `auth-visual` | Login/register/setup states → approved visual comparison | Exact reference required; semantic auth cases proceed independently. | Same-profile reference/actual/diff images when authority supplied. | .4/auth visuals **unassigned**; `blocked: D6-presentation`. |

### 4. `delivery.evaluation` — Git, retained evaluation and Config

Authority: A-contract, TASK-490.8/.9, accepted Systems identity/read boundaries; E-domain locates detailed evaluator/snapshot mechanisms. Sources: S-api `flakes.rs`, `commits.rs` and submodules, `systems.rs`; S-worker evaluation/config modules. Owner/scenario: **Delivery/D**.

| Obligation | Actor / initial state → action | Independent expected result | Authoritative observation | Tracking / status |
| --- | --- | --- | --- | --- |
| `ingestion` | Authored repository with distinguishable commits → poll, manual sync, valid/duplicate webhook, force push and accept rewrite | Each authored event maps to correct repo/branch/full SHA; deduplication does not multiply work; rewrite acknowledgement never reinterprets retained old identity. | Independent Git log/tree, commit/branch/evaluation rows and rewrite UI. | .7/.8; **integration-covered subset**: authored commit, smart-HTTP Git and manual sync preserve exact SHA in D. Poll, webhook, deduplication and rewrite variants remain `planned`. |
| `git-credentials` | Admin/Operator and credentialed fixture → create/test/rotate/delete SSH or HTTP credentials, sync private root/locked input | Right credentials reach intended origin; failures create no successful evaluation; readback/logs redact secrets; test does not become saved credentials. | Fixture requests, protected credential rows, job identity, sanitized browser/logs. | **Unassigned** within .8 boundary; Access owns form persistence; `planned`. |
| `identity` | Two exact source revisions/configurations → evaluate each and open Evaluation drawer | Full source/lock/target identity selects independently computed `.drvPath`; failed discovery/evaluation is not an empty successful inventory. | Independent evaluator invocation against authored source, evaluation/derivation/artifact DB, browser. | .7/.8; **integration-covered subset**: one exact commit/native attempt/derivation/output chain and independently authored nonce in D. Two-revision contrast, failure variants and browser bindings remain `planned`. |
| `queue-attempts-logs` | Real pending/running/failed evaluations → reorder, cancel/force-cancel, retry; open retained attempt/log stream | Queue action changes only selected work; stale producers cannot append to successor attempt; old attempt and its matrix/logs stay old after retry; visibility applies to history and streams. | Queue/attempt/log DB, worker process lifecycle, independent browser/SSE capture. | .8 extension **unassigned**; `planned`. |
| `config-inspector` | Real evaluated revision with authored option definitions and secret paths → inspect values, provenance, diff, module declarations/options search | Correct values/definition winners and revision; secrets redacted before storage/exposure; unavailable content is not fabricated by GET; bounded pages preserve one snapshot identity. | Authored Nix option facts, stored snapshot/provenance, browser and no new queue work on reads. | .9; `planned`. |
| `config-observation` | Authorized system with exact revision → explicit evaluation prerequisite, inspection/observation POST, request polling and by-ID read | Explicit action queues/reuses only intended work; Admin whole-commit boundary retained; stale/foreign request/snapshot cannot disclose or overwrite current selection. | Config request/observation/snapshot tables, worker records, browser query state. | **Unassigned** .9-adjacent; `planned`. |
| `historical-navigation` | Retained attempt and captured evidence; later retry/head movement → exact evaluation/compliance links and return | Full attempt UUID and captured system/bundle/policy version remain selected; navigation never substitutes latest/deployed evidence. | Attempt/snapshot rows, browser direct URL/reload/back/return and captured policy matrix. | **Unassigned** .8/.9 and Assurance caller coordination; `planned`. |
| `webhook-and-csrf-policy` | Untrusted ingress/cookie mutation outside an explicitly approved protection case → request | Public webhook authentication and remaining cookie-CSRF expectations need exact authority; do not accept unsigned implementation as policy. | HTTP, before/after commit/queue rows and external fetch observer. | .8 / Access authorization; `blocked: D4-auth-transport` for these exact security-policy cases. |

### 5. `delivery.build` — builder trust, scheduling and execution

Authority: A-contract, TASK-490.7/.10/.11. Sources: S-builder, S-wire builder/source artifact, S-api `builders.rs`; E-domain evaluator-contract pointer. Owner/scenario: **Delivery/D**.

| Obligation | Actor / initial state → action | Independent expected result | Authoritative observation | Tracking / status |
| --- | --- | --- | --- | --- |
| `registration-session` | Two separately provisioned builders → resolve ID, signed session, heartbeat, deactivate/rotate key/reconnect | Correct identity/capability and session only; invalid key, wrong path/method/body, stale timestamp or foreign ID gains no job authority. Key rotation invalidates obsolete authority as specified. | Builder/session/key DB, signed request outcomes, independent process identity. | .7/.10; **integration-covered subset**: two independent runtime keys, registered IDs and real API sessions/heartbeat; BuilderA job-session correlation in D. Negative authentication, rotation/deactivation and reconnect variants remain `planned`. |
| `claim-scheduling` | Real queued jobs with architecture/environment/capacity constraints → competing next-job GET/POST | Exactly one authorized claimant; unsupported evaluator/strategy/capacity cannot claim; server owns queue and API builder has no SQL connection. | Build jobs/reservations/sessions, independent builder stores and DB client list. | .10/.11; `planned`. |
| `server-derivation` | Fresh absent output → authorized manifest/full or delta derivation archive, start/build/progress/complete | Only job-authorized store paths transfer; foreign/extra path rejected; real builder realizes server-selected derivation and reports exact output. | Manifest, absent-before proof, real Nix store/build logs, job attempt and completion records. | .7/.10; **integration-covered subset**: authorized materialization, real offline realization, absent-before/present-after nonce output and successful exact job/session completion in D. Foreign/extra-path denial and other transport variants remain `planned`. |
| `verified-source` | Canonical server-bundled tracked source → capability check, source archive verification, re-evaluate/compare/build | Exact source/lock and evaluator yield authored derivation before realization; corrupt archive or derivation mismatch fails before build; sanitized repository URL is not fetch authority. | Independent source digest/NAR/lock/eval and builder process/store observations. | .10; `planned`. |
| `ordering-retry` | Real waiting/failed jobs → prioritize, move up/down, reorder, manual requeue and automatic retry | Authored ordering and eligible retry policy control new attempts; retry preserves original attempt/evidence and does not duplicate live claims. | Queue order, job history/attempts, Admin retry settings, builder claims. | .11; ordering variants **unassigned**; `planned`. |
| `cancel-fencing` | Running job then graceful/force cancel, dead worker, reassignment/session replacement → late progress/log/complete/fail/finalize | Canceled work stops; only current authorized lease/session may mutate job; fenced worker cannot complete successor; terminal state is not resurrected. | Actual process cessation, build-job/session/attempt records and rejected late request outcomes. | .11; `planned`. |
| `history-logs-metrics` | Real successful/failed/canceled attempts → Builds detail, logs/stream, builder metrics | Attempt identity and result/logs agree; viewer gets only authorized read; secrets are redacted; UI does not merge retries. Metrics describe actual worker workload. | Persistent attempt/log records, process observations, browser/SSE and sanitized logs. | .10/.11; read/metrics bindings **unassigned**; `planned`. |

### 6. `delivery.publication` — cache authority and cold retrieval

Authority: A-contract, TASK-490.12/.13; S-config/S-wire/S-api locate distinct supported boundaries. Owner/scenario: **Delivery/D**. Every positive copy/publication uses per-run output, proves destination absence first, and independently checks stored bytes/NAR/signature. A metadata Test result cannot prove upload or retrieval.

| Obligation | Actor / initial state → action | Independent expected result | Authoritative observation | Tracking / status |
| --- | --- | --- | --- | --- |
| `cache-configuration` | Admin with destination/environment bindings → save/rotate/remove secrets and assign destination | Exact destination/type/environment persists; masked readback does not erase retained secret; credential removal is explicit; encrypted storage contains no plaintext secret. | Cache destination/assignment rows and raw encrypted field inspection without logging values; independent access probe. | Access form **unassigned**; Delivery .12/.13 execution; `planned`. |
| `probe-discovery` | Admin and disallowed actor → unsaved/stored Test and Niks3 discovery including alias, scoped write/read/all | Discovery/Test obey intended target/TLS policy, merge secrets only in memory and persist no destination/assignment/job. Read success never attests write permission. | Fixture origin/DNS/redirect observations, DB before/after, sanitized diagnostic/browser result. | .12/.13 extension **unassigned**; `planned`. |
| `niks3-write` | Real builder output, separate write-plane endpoint → token then mTLS publication | Only valid write credential authorizes upload to intended cache; wrong token/CA/client identity fails without successful publication; read secret is not a write credential. | Cache service/object/NAR observations, build/publication rows, independent bytes/signatures. | .12; `planned`. |
| `niks3-read` | Cold capable agent with assigned published cache → public, mTLS and Basic retrieval; wrong credential/redirect/origin/legacy capability | Valid read plane retrieves exact signed output; invalid trust/auth fails closed; no write token/key leaks into heartbeat/read process; signed capability governs runtime settings. | Agent store absence/presence, native cache requests, heartbeat payload inspection with secrets suppressed, publication binding. | .12; Basic/capability/security variants need explicit tracking detail; `planned`. |
| `adapter-variants` | Fresh outputs and supported credentials → Attic native push/read, S3 copy/object access, Http/Nix configured copy/read | Each adapter's distinct publication/read authorization and signature boundary works; do not invent HTTP upload support for read-only service or repeat common lifecycle per adapter. | Adapter-native service/object logs, cold reader store and independently verified closure. | .7/.13; **integration-covered subset**: signed Nix-store publication over authenticated SSH to a previously cold destination in D. Attic/S3/HTTP, cold-agent retrieval and negative trust/auth variants remain `planned`. |
| `publication-jobs` | Real failed/queued publication → list/detail, retry/cancel and bulk operations | Only eligible intended jobs change; bounded retry does not fabricate success; cancel prevents later successful publication accounting; partial adapter failures remain visible. | Cache push jobs/history plus actual object existence and worker lifecycle. | **Unassigned** .12/.13 extension; `planned`. |
| `publication-authority` | Builder reports destination/output or derivation-closure publication → server verifies, issues agent runtime cache settings | Claimed success is insufficient; destination/output binding and verified cache identity authorize delivery; cross-environment/unverified destination cannot supply deploy credentials. | `cache_publication_reads`/publication DB mechanisms, independent cache retrieval, selected target heartbeat. | .12/.14; cross-boundary detail **unassigned**; `planned`. |

### 7. `delivery.activation` — agent, deployment, rollback and recovery

Authority: A-contract, TASK-490.14/.15 and accepted Systems decisions. Sources: S-agent/S-wire, S-api `systems.rs`, `deployments.rs`, S-worker deployment/system state/approval/canary mechanisms. Owner/scenario: **Delivery/D**.

| Obligation | Actor / initial state → action | Independent expected result | Authoritative observation | Tracking / status |
| --- | --- | --- | --- | --- |
| `agent-register-heartbeat` | Admin/module registers target and key, two real agents → startup/state/heartbeat, tampered/unknown-key reports | Agent belongs only to registered system; signed state records actual host/running path/generation; invalid report cannot alter state or obtain desired target/cache secrets. There is no separate public agent-registration route. | Independently gathered target facts, systems/state/heartbeat rows and signed responses; second-target isolation. | .7/.14; key lifecycle and negative ingress detail **unassigned**; `planned`. |
| `deployment-policy` | Manual/auto_latest/pinned systems with known revisions → choose deploy action, retry same intent, conflict intent | Exact eligible target selected; explicit auto-latest continuation/conversion intent respected; retries do not enqueue duplicate deployment or affect another target. Partial policy conversion versus queue failure must be independently specified before detailed case authorship. | Policy/request/deployment rows, heartbeat desired target, browser Deploy flow and actual target. | .14; request/partial-success extension **unassigned**; `planned`. |
| `approval-rollout` | Real approval/time-window/canary policy and multiple targets → authorized/unauthorized approval, phase advance/halt | Policy-selected approval authority and rollout gates govern real deployment; unmet/expired approval or halted phase cannot activate later target. Approval history identifies actor and selected commit/policy. | Approval/rollout/admission records and actual target paths over time. | **Unassigned** .14/.18 coordination; `planned`. |
| `activation-modes` | Built/published target absent from destination → ImmediatePersist then BootOnly, reboot | Immediate mode runs intended system; BootOnly changes next-boot selection while running and booted identities each retain their separately recorded pre-action values; reboot runs/boots selected target; other agent unchanged. | `/run/current-system`, `/run/booted-system`, independent boot-loader selection, boot ID and deployment/state rows. | .14; `planned`. |
| `failure-progress` | Real copy/activation failure or lost server → deployment-started/failed callbacks, status/history/events | Failure/progress reports match attempted target; failed copy/activation is never reported as successful running state; retry respects actual running state and does not double-activate. | Actual cache/activation process outcome, target path, deployment/event records, browser logs/history. | .14/.15; callback/status bindings **unassigned**; `planned`. |
| `rollback-retention` | Retained previous generation plus foreign/missing identity → rollback/default or exact generation, closure verification | Only exact authorized retained lineage can roll back; path alone or foreign generation is insufficient; target actually restores intended generation; history/snapshot lineage persists after source rewrite/GC. | Target generation/path, retained generation/deployment/snapshot rows, browser History/Config. | .15; retention/rewrite variants **unassigned**; `planned`. |
| `drift-recovery` | Local out-of-band activation and in-flight work → agent/server/builder restart, network interruption/rejoin | Report actual drift/local origin without inventing deployment proof; bounded current-version recovery reconciles real state, reclaims stale work and preserves completed history. | Target processes/store/boot IDs, DB histories/leases and journals; no same-version restart claim as Upgrade. | .15; `planned`. |

### 8. `assurance.scanning` — CVE/hardening execution and evidence

Authority: A-contract, TASK-490.16/.17, A-systems and A-continuity Sections 6–7, 22, 27–29. Sources: S-api `cves.rs`, `systems.rs`, `hardening.rs`, `scanning.rs`, `builders.rs`; S-worker scan/lease/inventory modules. Owner/scenario: **Assurance/A**.

| Obligation | Actor / initial state → action | Independent expected result | Authoritative observation | Tracking / status |
| --- | --- | --- | --- | --- |
| `cve-execution` | Real intended closure plus deterministic vulnerability data → system/flake/derivation/fleet scan | Scanner actually consumes exact closure; authored CVE/package set and clean control produce corresponding immutable observations, not fake success. | Independent package closure/data, real scanner process/output, scan/observation DB. | .16; all trigger entry-point bindings need tracking detail; `planned`. |
| `scanner-lease` | Two API builders with real queued CVE scan → claim/heartbeat/complete/fail and stale/oversize submissions | Only active owner lease accepts bounded results; stale/foreign completion cannot replace evidence; failure/retry retains distinction from clean. | Scan lease/attempt/observation rows, actual scanner and rejected request records. | **Unassigned** .16/.10 coordination; `planned`. |
| `current-historical` | Running A, built/head B, historical C; mapped/unmapped/ambiguous/absent observations → browse and refresh | Current follows actual reported mapped derivation, never head fallback; explicit history remains selected/read-only where specified; missing/failed evidence is not clean; newest failed attempt does not erase valid same-target completed evidence. | Real target identity plus exact scan/derivation/source rows; browser target/query and API. | .16; `planned`; continuity specification governs Current actions, not superseded SC1 gate. |
| `counts-inventory` | Authored CVE/package/system set and scoped users → summaries/grouped inventory/pages/detail/top systems/freshness | Counts use accepted units and scope; grouping/page totals do not count duplicate observations as distinct findings; failed/missing sources remain distinguishable. | Independent authored set arithmetic, exact scans/observations and browser fleet/system dashboards. | .16; advanced dashboard/inventory bindings **unassigned**; `planned`. |
| `scan-console` | Admin with real scheduled/failed/completed scans → queue/activity/detail, schedule PUT and archive PATCH | Selected schedule persists and yields eligible work; archive does not alter evidence truth; deployed/scan history distinguishes unscanned from clean. Non-Admin cannot mutate Admin console. | Schedule/archive/scan DB, worker scheduling and browser reload. | **Unassigned** .16 extension; `planned`. |
| `hardening` | Real target with authored hardened/weak units → eligibility, scan, inventory/current/history, justification | Findings reflect target unit facts; scan attempt and completed evidence remain distinct; exact target is not substituted by older/head evidence; scoped justification persists without rewriting technical scan result. | Target unit configuration and real scanner output; hardening scan/result/justification records, browser. | .17; `planned`. |
| `read-safety` | Hidden/foreign/stale target and unsaved triage draft → read inventory/candidates, refresh/paginate, delayed response | Authorize before source disclosure; GET enqueues no scan/build/deploy/plan/decision; late target response cannot retarget mounted draft; current evidence changes require explicit concurrency handling. | Protected row/queue counts, response identity, real browser draft/query state. | .16/.20/.29; read-race binding detail **unassigned**; `planned`. |

### 9. `assurance.policy` — executable policies, bundles, assignments and frameworks

Authority: A-contract, TASK-490.18/.19 and approved A-design states. Draft Compliance/cross-view documents are not blanket domain approval. Sources: S-api `deployment_policies.rs`, `compliance.rs`, `framework_requirements.rs`, `environments.rs`; S-worker enforcement/approval/time-window/CVE mechanisms. Owner/scenario: **Assurance/A**; actual rollout belongs to Delivery.

| Obligation | Actor / initial state → action | Independent expected result | Authoritative observation | Tracking / status |
| --- | --- | --- | --- | --- |
| `policy-lifecycle` | Authorized editor/Admin plus Viewer → create/edit/draft/trust/publish/use/delete and bulk delete | Intended lineage/version/semantic content persists; trust and publication remain separate; immutable used version stays historical; deletion eligibility protects references; denied operation changes none. | Policy/version/usage/audit and dependent evidence records, editor reload. | .18; trust/draft/delete/bulk details **unassigned**; `planned`. |
| `bundle-grouping` | Admin → bundle CRUD/version membership, grouping scheme CRUD, filter/open evidence drawer | Exact authored policies/requirements grouped without changing technical evidence; deletion protects assignments/history; drawer binds exact selected system/bundle/version. | Bundle/version/membership/grouping tables and actual evidence, browser identity. | **Unassigned** .18-adjacent; `planned`. |
| `assignment-resolution` | Environment/system assignments with include/exclude/override inputs → preview, commit/update/remove, effective-policy read | Preview writes none; stored scope/version and approved precedence produce independently authored effective policy set; environment and host do not silently exchange identity. | Assignment/version/membership DB, independent expected set, enforcement result. | .18; `planned`; draft-only precedence conflicts require case-specific clause review. |
| `enforce-report-only` | Real passing/failing input and Enforce/ReportOnly assignments → evaluate and request deploy | Enforce failure blocks admission; ReportOnly records same technical outcome without blocking; missing/failed evidence follows accepted policy, not success default. | Evaluation decision/results, deployment admission records and actual target non/activation. | .18; `planned`. |
| `policy-kinds` | Authored native/expression/package/CVE/agent/approval/time-window/canary conditions → evaluate supported policy configuration | Each supported execution boundary gets real positive/negative input; technical result and gate meaning remain separate; no parser-only proof of execution. | Independent Nix/target facts, policy evidence/result DB, gate/rollout observer. | .18 broad boundary; per-kind execution matrix **unassigned**; `planned`. |
| `framework-catalog` | Imported STIG/framework release → list versions, search requirements/children/mapped policies, bundle coverage | Authored external rule/requirement identities and hierarchy survive; mappings distinguish policy evidence from framework coverage; search/filter scope does not drop version identity. | Independently parsed source catalog, framework/requirement/mapping DB, browser. | .19 catalog; extra framework/coverage bindings **unassigned**; `planned`. |
| `mapping-recovery` | Operator/Admin with exact requirement version and unresolved historic release → mapping CRUD, Admin recovery | Only intended mapping/version changes; failed/mismatched source recovery leaves unresolved version honest; startup can continue without inventing missing source digest/evidence. | Requirement mappings/version/digest rows, source file digest and recovery result. | **Unassigned** .19-adjacent; `planned`. |

### 10. `assurance.remediation` — triage, risk and POA&M continuity

Authority: A-continuity (including Section 32), A-contract, TASK-490.20/.21/.27/.28/.29 and corresponding approved design states. Sources: S-api `poam.rs`, `acceptance_register.rs`, `cves.rs`; S-worker `services/poam.rs`, `queries/{poam,acceptance_register,cves}.rs`. Owner/scenario: **Assurance/A**. Start from real evidence created earlier in A, not seeded outcomes.

| Obligation | Actor / initial state → action | Independent expected result | Authoritative observation | Tracking / status |
| --- | --- | --- | --- | --- |
| `triage` | Authorized Operator, Viewer/nonmember and real finding → system/fleet single/batch triage and justification/revoke | Exact canonical CVE/package/system or environment scope, actor and justification persist; denied/stale mutation changes none; batch applies only intended compatible findings. | Stable finding/disposition/history/audit and immutable observation rows; browser selection. | .20; `planned`. |
| `finding-waiver` | Real policy failure plus authorized requester/approver → request/list/detail/decide waiver | Source-owned waiver lifecycle/authority retained; acceptance does not make technical evidence pass or remediate target; scope and observation/policy version stay exact. | Waiver/finding/version/audit records and unchanged target/scanner evidence. | .27; `planned`; no global authority blocker. |
| `acceptance-register` | Policy waiver and CVE accepted risk visible in register → filter/open/review/renew | Typed source ID and source revision retained; renewal is audited new review with server-clock 90-day deadline and preserves required origin/scope; forbidden source state cannot renew. | Source waiver/disposition records, successor/audit links and browser reload. | .28; `planned`, A-continuity Section 32 is authority. |
| `acceptance-conversion` | Selected acceptance plus current real finding → convert, duplicate retry, concurrent conflict | Atomic source-authorized plan creation/compatible reuse and selected acceptance replacement; rollback on failure; one replacement event; other host/environment decisions unchanged. Never browser revoke-then-create. | Transactional source/plan/link/audit before/after, concurrent request results. | .28; `planned`. |
| `poam-plan` | Real failed policy/CVE evidence → create compatible plan, edit owner/details, note/milestone/link/unlink/transition | Exact finding family and opened-against evidence preserved; accepted fields/transitions persist; wrong-family/foreign link or invalid transition changes none; cancel/read never writes. | Plans, immutable baseline/link/assignment/notes/milestones/history and browser reload. | .21; `planned`. |
| `continuity` | Same CVE/package across revisions/versions, environment members entering/leaving/moving, host override → advance real deployments/scans | Stable finding/plan identity survives evidence changes; prospective environment default and host override resolve independently; clean/unknown/affected remain distinct; technical remediation is not a disposition. | Independently authored membership/revision sequence, findings/evidence/dispositions/links and target scans. | .20/.21/.29; cross-revision/membership matrix detail **unassigned**; `planned`. |
| `verification-closure-recurrence` | Open plan, old/failing/unknown then newer Current clean evidence; later recurrence → verify/close/reopen | Fresh Current evidence required; unknown/failure cannot verify; closure follows accepted evidence and authority; return before closure resumes same episode, recurrence after closure preserves closed episode/history and follows normative recurrence behavior. | Actual target/scans, baseline/current evidence IDs, verification/history/plan state; denied stale results. | .29; `planned`. |
| `register-navigation-concurrency` | Real scoped records → dashboard/watchlist/rollups/relationships/compatible/assignees, drawer and cross-view return; stale concurrent write | List/filter totals match authored scope; hidden/foreign records non-disclosing; exact evidence/plan stays selected through return; stale writes conflict without losing retained draft or duplicating decisions. | Scoped DB selections, source versions/locks, URL/reload/back and no-write reads. | .21/.28/.29; aggregate/caller details **unassigned**; `planned`. |

### 11. `assurance.interchange` — import, export and independent consumers

Authority: A-contract and TASK-490.19/.22; A-continuity for source identity/scope. Draft `docs/design/CrystalForge/uploads/crystal-forge-xccdf-interchange-profile-v0.1.md` supplies questions, not approval. Sources: S-api `compliance.rs`, `register_export.rs`, `acceptance_register.rs`, `poam.rs`, `cves.rs`, server compliance/interchange/schema and register export services; S-ui `io_menu.rs` and domain export consumers. Owner/scenario: **Assurance/A**.

| Obligation | Actor / initial state → action | Independent expected result | Authoritative observation | Tracking / status |
| --- | --- | --- | --- | --- |
| `stig-import` | Authorized user with DISA Anduril XCCDF XML/ZIP → preview/import; malformed/oversize/invalid payload | Preview writes nothing; authored rule identities/count from independent parser persist only after valid import; rejected import is atomic; role/body limits apply before protected writes. | External source parser/schema plus framework/bundle/policy DB before/after and browser import state. | .19; `planned`. |
| `policy-interchange` | Real authored policy/version set → export/download, preview/import into isolated receiver state | Download parses independently; selected semantic records/scope round-trip; validation error leaves receiver unchanged; imported trust cannot be inferred from file presence. | Parsed payload/schema, source/receiver DB and actual browser download. | .22; `planned` for accepted round-trip subset. |
| `native-reconciliation-details` | CF-native identities/digests with exact match, lineage/version conflict or unverified signature → preview/import | Exact draft-only reconciliation/trust conflict outcomes require approval; existing implementation/tests cannot select expectation. | Independent source digest/identity comparison, receiver DB transaction results. | .22; `blocked: D5-interchange` only for these details. |
| `server-exports` | Real scoped plans/acceptances/CVEs/bundles/assignments/policies → register CSV/XLSX/OSCAL JSON/XML, POA&M/acceptance CSV/XLSX, CVE CSV, XCCDF and policy exports | Independent parsers/schema validate actual files; exact authored records/IDs/scope and source versions included; hidden/unselected data omitted; export does not mutate domain records. | Downloaded bytes parsed externally, source-selection DB and no-write observation. | .22; `planned`. |
| `client-exports` | Real view/filter selection → client CF-JSON/CSV/SARIF/OSCAL/print HTML where exposed | Actual client output preserves selected identity/scope and honest unavailable evidence; no fabricated pass or server-export parity assumed. | Browser-produced file/print document, independent parser and selected real source records. | **Unassigned** .22-adjacent; `planned`. |

### 12. `upgrade.continuity` — version transition and compatibility

Authority: A-contract/TASK-490.23, with release matrix pending D2. Sources: S-router startup/migrations, `packages/default/crates/cf-server/migrations/`, S-wire, S-agent/S-builder, retained generation/snapshot/evidence DB mechanisms. Owner/scenario: **Upgrade/U**.

| Obligation | Actor / initial state → action | Independent expected result | Authoritative observation | Tracking / status |
| --- | --- | --- | --- | --- |
| `persisted-transition` | Real supported older server creates users/sessions/resources/credentials/history → install current package and start normally | Original IDs, usable credentials, history and evidence relationships survive forward migrations; one new real operation succeeds; no synthetic old-schema/current-seed substitute. | Old/new independent DB reads, migration set, real sign-in/new operation and preserved evidence. | .23; `blocked: D2-upgrade`. |
| `mixed-version` | Supported old agent/builder against upgraded server → signed heartbeat/session/claim/build/delivery | Declared compatibility window works; unsupported capability receives explicit safe refusal without corrupting claims or leaking runtime credentials. | Real old binaries, wire responses, job/target/store and DB state. | .23; `blocked: D2-upgrade`. |
| `transition-recovery` | Old-version in-flight work and completed history → migrate/restart/recover | Recovery uses migrated state without duplicate successful work or false terminal success; migration failure cannot be reported as successful upgrade. | Old/current process outcomes, leases/attempts/history and target identity. | .23; `blocked: D2-upgrade`; same-version recovery remains .15. |

### Browser routes, forms, drawers and navigation bindings

All route variants in S-ui `routes.rs` are accounted for collectively below. Query parameters carry identity, filters and return state; testing a page mount does not prove its forms or domain values. Design component names locate approved states, not proof that all states in a draft are accepted.

| Routes / surfaces | Actual consumers and interactions to bind | Owner / scenario | Status | Tracking |
| --- | --- | --- | --- | --- |
| `/login`, `/register`, `/setup` | `views/{login,register,setup}.rs`; local forms/OIDC redirect, validation, first-admin and setup transitions | Access L/O | Selected bootstrap Integration-covered; disabled-account login denial covered at `c254b1c5`; remainder planned; visuals D6. | .1/.2/.3/.6 |
| `/dev/login` | `views/dev_login.rs`; conditional developer identity selector | Existing Rust/module boundary verification | Developer-only flow out of product integration scope; release-mode rejection remains lower-level. | Not a product integration selection |
| `/`, `/style-guide`, `/:..route`, shell/profile menu | `views/{dashboard,style_guide,not_found}.rs`, `components/layout/`, onboarding coach/tours/setup; sidebar collapse, breadcrumbs, keyboard/focus, notification tray | Access L; domain dashboard values D/A | Planned. Style guide is a component showcase, not domain evidence. | .4; domain selections in obligation tables |
| `/profile` | `views/profile.rs`; profile/preferences/notification choices and session list/revoke/revoke-all/sign-out | Access L/N | Planned except selected sign-out/session cases. | .2/.4/.5 |
| `/admin` | `views/admin.rs`; users/roles/OIDC/jobs/audit/server sections; create/edit/disable user, mappings, retry/classification and runtime diagnostics | Access L/O | Selected Viewer denial and seven Admin-form cases passed again at verified `bea0c299`; remaining bindings planned. | .2/.3/.6; diagnostics/audit details unassigned |
| `/environments?:..query` | `views/environments*.rs`, `components/environments/`; create/edit/remove/policy dialogs, validation, list/query filters | Access L; assignments Assurance A | Planned. | .3; assignment selections in obligation tables |
| `/flakes?:..query` | `views/flakes*.rs`, `components/flake/`; add/edit/delete, credentials/Test, refresh/sync/accept rewrite, explorer/source/module/diff and scan actions | Access L forms; Delivery D; scanning Assurance A | Planned. | Access forms unassigned; .8/.9; scans .16 |
| `/evaluations`, `/evaluations/:commit_id`, `/evaluations/:commit_id/attempts/:attempt_id` | `views/evaluations.rs`, `views/evaluations/{drawer_snapshot,policy_matrix,policy_captured,evidence_navigation,log_pane}.rs`; queue actions, logs, exact attempt matrix and evidence-return links | Delivery D | Planned. | .8/.9; exact retained-attempt/stream bindings unassigned |
| `/builders`, `/builds?:..query` | `views/{builders,builds}.rs`, `components/{builders,builds}/`; resource/key/environment dialogs, worker/detail/queue panes, retry/cancel/reorder/log stream | Access L resource forms; Delivery D | Planned. | Access resource forms unassigned; .10/.11 |
| `/caches` | `views/caches.rs`, `views/caches/{niks3,metrics}.rs`; add/edit/remove/credential/Test/discovery/assignment modals, publication queue and metrics | Access L persistence; Delivery D | Planned. | Access persistence unassigned; .12/.13 |
| `/systems?:..query`, `/systems/:id?:tab&:poam&:config_mode&:revision&:generation&:deploy_generation&:cve_target&:cve_mode` | `views/systems*.rs`, `views/system_detail.rs`, `components/system/`, modals; Overview/Config/Deploy/Compliance/CVEs/Hardening/History/Logs, preview, edit/key/remove/SSH helper, sync/rollback/generation rollback/auto-latest prompt | Access L resource forms; Delivery D and Assurance A | Planned; explicit tab/target/history binding required, not one global revision. | Access resource forms unassigned; domain selections in obligation tables |
| `/cves?:..query`, `/scanning` | `views/{cves,scanning}.rs`, `components/cve/`; package-first inventory/details, batch selection/triage, source/freshness, scan dialogs/schedule/archive and exports | Assurance A | Planned. | .16/.20/.22; console/advanced binding gaps unassigned |
| `/deployment-policies` | `views/policies.rs`, `components/policy/`; editor/refine, bulk remove, grouping, interchange, trust/publication/usage where exposed | Assurance A | Planned. | .18/.22; lifecycle variants unassigned |
| `/compliance?:bundle&:version&:system&:policy&:poam&:view` | `views/compliance.rs`, `components/compliance/`; catalog/version/filter, bundle editor, assignment/preview, requirement/mapping, import, evidence drawer and finding/POA&M links | Assurance A | Planned. | .18/.19/.21/.22; framework recovery/bundle lifecycle variants unassigned |
| `/compliance/evaluations/:commit_id/attempts/:attempt_id?:bundle&:version&:system&:policy` | `views/compliance/evaluation_evidence.rs`, `evidence_controls.rs`; captured attempt view, exact versions, return to Evaluation, unavailable historic state | Delivery D originating navigation; Assurance A evidence semantics | Planned. | Caller selection unassigned |
| `/poams?:..query` | `views/poams.rs`, `views/{poam_api,register_api}.rs`, `components/poam/`; scoped remediation/acceptance registers, details, owner/notes/milestones/link/verify/close/reopen, renew/convert, export tray | Assurance A | Planned; distinct policy/CVE source versions and return destinations required. | .21/.27/.28/.29/.22 |

Common drawer focus/close behavior belongs to Access; each Evaluation/System/Compliance/POA&M caller still proves its exact identity, permission, nesting and return target in D/A. Loading, empty, validation, error, stale and unavailable states are case-specific; no blanket design blocker replaces accepted semantic checks.

### Module/configuration discovery bindings

These bindings expand the `module-runtime` obligation and the relevant domain
rows; they are not extra covered rows. Source: S-config's module options,
serialization, scripts, units and assertions. Pure option evaluation uses the
existing lower-level module/component mechanism; the [component catalog](flake-checks.md#current-inventory-and-commands)
and [verification guide](../../agents/verification.md#consolidated-component-verification)
record its consolidated sources and commands. Only real runtime behavior earns
integration proof.

| Declared configuration surface | Required boundary / primary owner and planned observation |
| --- | --- |
| `enable`, `server.enable`, `build.enable`, `client.enable`, `local-database`, `database`, `env-file`, `configPath`, `log_level` | Access installation/config: generated effective settings, secret-file ownership and service role separation; local socket/migrations subset executed only. Remote DB/file-password and disabled/combined role variants remain unassigned/planned. |
| `server.auth_mode`, OIDC client/secret-file/group mapping, bootstrap, session throttle/retention | Access L/O: correct conditional auth routes and runtime session lifecycle; invalid/conflicting OIDC secrets rejected in module configuration. Public runtime health must not disclose secrets. |
| `flakes`, `auth` SSH/netrc/known-hosts, evaluator workers/memory/discovery/idle/overall timeouts | Delivery D: credentialed exact root/input source; resource/deadline limits cannot change successful source identity or leave work falsely successful after cancel/timeout. Source/evaluator resource fault variants are unassigned/planned. |
| `build` API mode/server/key, strategies/source delivery, IFD, concurrency, sandbox/offline/substitutes and systemd resource limits | Delivery D: ordinary module rejects removed direct-DB mode and unsupported verified-source settings; runtime API builder has independent key/store and obeys realization/cancellation limits. No synthetic build success. |
| `cache`, encryption key file, signing, compression/filter, retries, S3/Attic/Niks3 auth; deployment signature trust | Delivery D: effective secrets and trust survive config loading without store/log leaks; build/push/read use separate intended credential plane and exact output/destination. Test/probe private-target exceptions remain explicit fixture settings. |
| `deployment`, `systems`, `environments`, `client` server/key and startup delay/polling | Delivery D and Access resource forms: configured identity/defaults agree with persisted resources; runtime target/policy/activation/retry behavior observed on actual target. Serialization text alone is insufficient. |
| `vulnix`, `hardening`, server automatic hardening scans and worker units | Assurance A: enabled real worker consumes intended input, times out/fails/retries honestly and records actual evidence; scanner settings do not authorize fake completed evidence. |
| `server.notificationEmail.*` | Access N: endpoint/sender/token-file/delivery permission/digest/retry settings govern actual receiver delivery; disabled external delivery sends none. |
| `server.trust_forwarded_builder_https`, `trustedProxyCidrs` | Delivery D builder transport: explicit direct-peer trust and HTTPS gate; forwarded headers from an untrusted peer grant no secure-session authority. Module rejects enabled trust with empty CIDRs. Detailed proxy transport case is unassigned/planned. |
| `dashboards` datasource/provisioning and service setup | Supported optional module configuration: retain ordinary configuration assertions and credential-safe datasource wiring; runtime dashboard provisioning obligations need explicit tracking selection. Incidental historical Grafana/SQL-view assertions do not automatically become integration requirements. |

### Supported variants and explicit exclusions

| Surface | Discovered supported variants / disposition |
| --- | --- |
| Authentication | `local`, `oidc`; Admin/Operator/Viewer. Debug-only dev login is `out of scope` for production integration; release rejection remains a lower-level obligation. |
| Build/source | `ServerDerivation`, `SourceReEvaluateVerified`; `ServerBundledArchive` is the only accepted delivery for verified-source contract v1. `None` means not applicable; reserved `LocalGitWorktree` / `BuilderFetchPublicInputs` modes are rejected, not supported alternatives. Derivation equality does not promise output reproducibility. |
| Caches | `S3`, `Attic`, `Niks3`, `Http`, `Nix`; Niks3 write token/mTLS and separate public/mTLS/Basic read auth/capabilities. Test/discovery, upload, cold read, signature trust and environment binding are distinct obligations. |
| Deployment | `manual`, `auto_latest`, `pinned`; `ImmediatePersist`, `BootOnly`; actual running, booted, selected boot and retained generation identities are separate. |
| Policy/evidence | `Enforce`, `ReportOnly`; policy/bundle trust/publication/version and assignment are separate; CVE/hardening attempt versus completed evidence, Current versus explicit historical, stable finding versus observation versus risk decision versus plan are separate. |
| Notifications | `InApp`, `Email`, `Both`; per-user preference/delivery lifecycle and receiver proof, not a mocked send success. |
| Interchange | XCCDF 1.2 XML/ZIP and policy interchange; server/client export variants listed above. CKL import is `out of scope` (no importer); draft-only proposed features do not become supported surfaces. |
| Historical test artifacts | Mocked responses, seeded successful scans/builds/publications, incidental Grafana checks and baseline-approval tooling are `out of scope` as integration proof. Retired code is not a requirement source. |

### Inventory totals, missing assignments and next runnable milestone

Counts below count **obligation rows**, not endpoints, test cases or percentage coverage. Route and browser indexes overlap these obligations and must not be added to them. A row can contain several variants, so even an Integration-covered row is limited to its named executed subset.

| Contract family | Rows | Integration-covered subset rows | Planned | Blocked |
| --- | ---: | ---: | ---: | ---: |
| `access.lifecycle` | 7 | 3 | 2 | 2 |
| `access.configuration` | 8 | 2 | 5 | 1 |
| `access.shell` | 5 | 1 | 3 | 1 |
| `delivery.evaluation` | 8 | 2 | 5 | 1 |
| `delivery.build` | 7 | 2 | 5 | 0 |
| `delivery.publication` | 7 | 1 | 6 | 0 |
| `delivery.activation` | 7 | 0 | 7 | 0 |
| `assurance.scanning` | 7 | 0 | 7 | 0 |
| `assurance.policy` | 7 | 0 | 7 | 0 |
| `assurance.remediation` | 8 | 0 | 8 | 0 |
| `assurance.interchange` | 5 | 0 | 4 | 1 |
| `upgrade.continuity` | 3 | 0 | 0 | 3 |
| **Total** | **79** | **11 (Access and Delivery executed subsets; not complete rows)** | **59** | **9** |

Existing Rust/component/DB assertions remain lower-level evidence, not product
integration coverage. The 11 rows contain passed subsets only. Failing runtime
variants (P3/TASK-492 and TASK-496) remain explicit within their row; they are
not extra rows or silently covered by a positive subset. Exclusions are not
added to the 79 required/discovered obligations.

The known P3/TASK-492 failure is a variant within the partially covered module
runtime row, not another row to add to these totals. It remains visible in
`journals/controller/failed-units.txt` at the 23-case execution revision. A
passing selected Access aggregate does not establish that every module unit
started successfully or resolve this separate product defect.

Discovered **tracking-selection gaps** include resource forms omitted by .3; session remainder; setup/config-health/audit; Git credential lifecycle; evaluation ordering/cancel/log/retained-attempt navigation; Config request/observation lifecycle; builder key lifecycle/metrics/order; cache discovery/probes/secret merge/read capability/queue controls; agent ingress/callback/retained rollback; approval/canary phases; scanner lease/console/archive/schedule; bundle/grouping/version/trust/deletion lifecycle; framework mapping/recovery; CVE cross-revision/environment-membership concurrency; mixed register aggregates; client exports. Rows assign a primary owner/scenario, but the coordinator must explicitly extend/select tracking scope before implementing those details. No task is created or changed by this inventory.

**Next controller-only milestone:** environment success, duplicate validation and
remaining form details in L. The semantic cleanup preserved all 23 existing
Access cases and the normal aggregate passed again at `590ee94c`; this is no
longer a pending rerun. Tracking: TASK-490.3. These workflows have no Delivery-role
dependency. D3 blocks only the unspecified failed-save/presentation states.

**Delivery foundation:** the concrete roles and D's source/evaluation/real-build
proof now exist and executed locally. Tracking: TASK-490.7. Later ingestion,
source-verification, scheduling, publication variants, activation and recovery
extend compatible sections of D using its receipts, not a new packet VM or Access
runtime state. TASK-495 is the separately authorized offline-realization product
fix; TASK-496 records heartbeat classification falling back to full state after
a database error. Neither positive reporting nor the build proof repairs that
classification defect. Case-local decisions and P3/TASK-492 remain separate.

Research located supported surfaces; it did not establish coverage. Execution
records distinguish the post-cleanup Access pass, successful corrective hosted
pipelines, and the new Delivery proof. Documentation validation is not
integration evidence.

## Product defects and design gaps

| ID | Kind | Summary | Contract | Task |
| --- | --- | --- | --- | --- |
| P1 | Historic product configuration mismatch; repair integrated and verified in the Access VM | At `fc930335`, the normal module's local selection was missing from TOML and the server unit had `AUTH_MODE=unset`; auth was not reached. TASK-490.26 now sets the unit variable from the module option for local and OIDC. NixOS evaluation and the rebased Access runtime setup-status/browser flow verify local mode; OIDC option evaluation verifies its unit value. | `access.lifecycle.local-bootstrap` | TASK-490.26 (`76e51464`) |
| P2 | Historic product defect; repair integrated and verified in the Access VM | At `fc930335`, `/run/postgresql` serialized as an empty-authority URI and server startup failed before migrations. TASK-490.25 now percent-encodes socket host and URI components in active cf-config `DatabaseConfig::to_url`; both client parsers round-trip socket and encoded values, and the rebased Access VM starts through the production local socket and applies migrations. | `access.lifecycle.local-bootstrap` | TASK-490.25 (`52128b3a`) |
| P3 | Independent incidental product defect; not an Access gate failure | On fresh boot the `crystal-forge-postgres-jobs` oneshot races database-role setup and logs `FATAL: role "crystal_forge" does not exist`. The setup script creates the role later; server startup, migrations and all Access cases pass. This is not authorized for repair in this overhaul. | VM journal; `access.lifecycle.local-bootstrap` still passes | TASK-492 |
| G1 | Design gap | No approved design component for `/login`, `/register`, or `/setup`. | `access.lifecycle` visual cases | None; semantic assertions only |

## Legacy retirement map

The owner explicitly authorized source retirement in TASK-490.24 on 2026-10-18,
before replacement coverage is complete. This supersedes retention until all
four owners pass. The dispositions below describe source removal or relocation,
not coverage. Accepted requirements remain in the obligation map. Legacy test
mechanics, mocked responses, seeded successes, and incidental Grafana checks
do not become mandatory requirements merely because they once ran.

Final minimization rule: `checks.x86_64-linux` contains exactly four TOTAL
attributes: `integration-access`, `integration-assurance`, `integration-delivery`,
and `integration-upgrade`. Every separate verification package or CI test job
must have a distinct necessary purpose. Preserve useful assertions, assign
primary execution ownership, and consolidate or justify duplicate execution.

Recorded inventory after the source-retirement milestone: one attribute, `integration-access`, versus
27 before retirement. That historical run passed 16 cases; the later `c254b1c5`
run passed 23 with seven Admin-form additions. The post-cleanup rerun is pending.
Delivery, Assurance, and Upgrade
are absent. XML fixtures alone do not constitute an Assurance owner. This is
not final completion. Final acceptance also needs complete accepted coverage,
passing aggregates, justified scenario isolation, and measured execution and
resource costs. Multiple attributes can share a derivation; a smaller name
count does not by itself remove duplicate work.

Verify the final full inventory explicitly:

```bash
nix eval --json .#checks.x86_64-linux --apply builtins.attrNames
nix eval --json .#checks.x86_64-linux --apply 'c: builtins.attrNames c == [ "integration-access" "integration-assurance" "integration-delivery" "integration-upgrade" ]'
```

The predicate must return `true` for final acceptance; successful evaluation
with `false` is not completion. No placeholders may fill missing names.

| Legacy check or job | Kind | Disposition | Replacement owner / scenario or reason |
| --- | --- | --- | --- |
| `checks/web-ui` | Integration, browser | retired by owner decision | Accepted Access, Delivery, and Assurance obligations remain gaps except the executed Access subset. Of 181 legacy steps, 105 used route mocks; these are not integration proof. Useful host workflows, manifest, design fixtures, generator and drawer tools survive under `packages/devScripts/web-ui/`; VM phases, baselines and approval tooling are retired. |
| `checks/niks3-ui` | Duplicate integration | retired | Repeated legacy browser/cache phases; accepted publication proof belongs to planned Delivery. |
| `checks/web-ui-reconciliation` | Fully mocked browser | retired as obsolete implementation | No real backend proof. Accepted import behavior remains planned Assurance work. |
| `checks/integration` | Integration | retired by owner decision | Accepted ingestion and activation obligations remain planned Delivery work. Incidental Grafana/SQL-view assertions are not mandatory product integration requirements. |
| `checks/oidc-auth` | Shallow integration | retired by owner decision | Real authorization-code/session/mapping proof remains planned Access work. |
| `checks/niks3-cache` | Integration | retired by owner decision | Planned Delivery owns publication and builder boundaries; seeded queue success is not carried forward as proof. |
| `checks/niks3-write-probe` | Integration | retired by owner decision | Accepted connection Test boundary remains planned Delivery work. |
| `checks/attic-metrics` | Integration | retired by owner decision | Accepted adapter observations belong to planned Delivery variants. Production adapter package remains. |
| `checks/attic-publication` | Component fixture | retired by owner decision | Accepted native publication belongs to planned Delivery variants. |
| `checks/nixos-discovery-reliability` | Nix contract | consolidated | `modules/nixos/crystal-forge/tests/discovery.{sh,py}`, direct runtime CI with independent temporary Nix stores. |
| `checks/server-regressions` | DB-backed Rust | consolidated | Existing `devScripts.state-machine-test` owns unique ignored/integration selections and both populated migration rehearsals; migration-prefix checks never satisfy Upgrade. |
| `checks/builder-evaluator-packaging` | Packaging | consolidated | `modules/nixos/crystal-forge/tests/packaging.nix` pure module evaluation plus `packaging.sh` direct installed-wrapper/service probes; no nested validation derivation. |
| `checks/web-ui-test-runner` | Dev tooling | consolidated | `packages/devScripts/web-ui/tests/web-ui-test-runner-test.sh`, direct CI selftest under `nix develop`. No backend integration claim. |
| `checks/run-ui-dev-db-check` | Dev tooling | consolidated | Existing `packages/devScripts/{db-usability-check-test,db-only-start-test}.sh`, direct `component-tests` CI under `nix develop`. |
| `checks/okf-knowledge` | Documentation | relocated | `packages/okf-knowledge`; validation, provenance and diagram audits preserved. |
| `checks/ui-screenshots` | Mocked screenshots | relocated | `packages/ui-screenshots`; design iteration aid without semantic assertions. |
| `checks/config-inspector`, `config-observer`, `evaluator-snapshot-isolation`, `verified-source-evaluator-parity`, `nixos-options-metadata`, `niks3-ca-bundle` | Nix/Rust contracts | consolidated | Core-only evaluator `postCheck` fragments, explicit normal `cf-config` library selection and Go/Rust CA comparison; ordinary metadata package assertions. [Exact homes](flake-checks.md#current-inventory-and-commands); no D integration proof. |
| `checks/builder-cve-contract`, `oscal-export`, `xccdf-schema`, `stig` | Contracts/unit | consolidated | Direct module boolean evaluations, ordinary OSCAL fixture check phase and actual installed XCCDF writer/schema command. [Exact homes](flake-checks.md#current-inventory-and-commands); no A integration proof. |
| `checks/test-keys` | Unused placeholders | removed | No executable consumers; not a coverage obligation. |
| Legacy browser XCCDF XML V1R1 and V1R2 | Immutable fixtures | relocated | `checks/integration-assurance/fixtures/xccdf/`; no `default.nix` and no placeholder check. |
| CI `cve-processing-test`, `dashboard-visibility-tests`, `state-machine-tests` | DB-backed Rust | existing DB mechanisms; consolidated selections | State-machine workflow owns 286 unique ignored library tests, fourteen integration targets/231 cases and both migration rehearsals in fresh invocation-owned PostgreSQL. Includes the two TASK-496 regressions. Six ignored duplicates and 94 normal-library selections removed; two duplicate `composite_policy` executions removed from CVE job. Dashboard unchanged. Local offline continuation passed 517 cases; current hosted evidence remains pending. |
| CI `legacy-flake-check`, `web-ui-check`, `web-ui-baseline-candidates` | Legacy/manual jobs | removed | Sources and candidate-baseline workflow retired; no replacement pass claimed. |
| CI `web-ui-screenshots-mr-comment` | Reporter | already removed in early transition | No replacement comment bot. Access exports its own evidence. |

### Current GitLab selection

The source-retirement milestone removed optional manual legacy jobs and first
relocated lower-level definitions into packages. The latest owner instruction
supersedes that one-for-one relocation: all fifteen public `verify-*` definitions
are removed. Unique assertions now use ordinary Rust/component/package tests,
direct CI commands and the existing isolated state-machine workflow. The
[component catalog](flake-checks.md#current-inventory-and-commands) maps all fifteen
successors and the [verification guide](../../agents/verification.md#consolidated-component-verification)
gives direct commands. No registry, new aggregate framework or fifth check is
introduced. Historical commands
below remain records of what actually ran at their recorded revisions.

| GitLab job | Selection now | Notes |
| --- | --- | --- |
| `integration-access` | Automatic and required on merge requests and `main` | Builds the normal owner aggregate `checks.x86_64-linux.integration-access`. Exports `integration-evidence/integration-access/` even when the build fails. |
| `integration-delivery` | Automatic and required on merge requests and `main` | Builds the normal native Delivery aggregate and exports `integration-evidence/integration-delivery/`, including failed-run evidence. Inherits the existing owner job's resource/cache/exit-status rules. |
| `legacy-flake-check`, `web-ui-check`, `web-ui-baseline-candidates` | Removed | No optional legacy execution remains. |
| `component-tests` | One explicit job replaces the six-entry package matrix | Builds core, metadata, OSCAL and docs; runs pure module evaluations, independent-store discovery, installed wrappers/schema and tool selftests directly. Fifteen verification wrappers removed. |
| `cve-processing-test`, `dashboard-visibility-tests`, `state-machine-tests` | Three automatic disposable-DB paths | Expanded state-machine invocation owns consolidated regressions on temporary PostgreSQL port 35433 with `SUPERUSER CREATEDB`, stats preload, both DB URLs in SQLx shell and one Cargo target directory. CVE exact duplicates removed; dashboard unchanged. |
| `coverage-check`, `complexity-check`, `auto-bump-patch` | Unchanged | Release and version behavior is unchanged. |

Executed integration subsets are twenty-three Access cases and eight initial
Delivery cases. GitLab schedules both normal owner aggregates automatically.
Exact results and revisions are recorded separately. The following
accepted obligations have **no integration owner coverage** until implemented;
there is no longer a manual legacy fallback:

- Access: registration policy, cross-environment scope, OIDC, remaining resource/environment forms and nonempty assignments, preferences, notifications, shell behavior, and authorization beyond the selected admin-only operations.
- Accepted browser and design behavior beyond the selected Access proof. Legacy mocked steps are not automatically mandatory requirements.
- Remaining Delivery obligations: ingestion variants, verified-source execution, scheduling/cancellation/fencing/retry, Niks3/Attic/S3/HTTP adapters, cold agent retrieval, activation, rollback and recovery. The positive source/native-build/local signed-publication foundation is not complete Delivery coverage.
- All Assurance obligations.
- All Upgrade obligations.

This is an explicit owner-authorized coverage gap during construction. Source
retirement removes implementations; it does not remove accepted obligations or
claim their replacement passed.

Final TASK-490.24 completion still requires the two absent owners, complete
accepted coverage, four passing normal aggregates, the exact four-name
inventory/CI allowlist, execution-cost evidence, and duplicate-execution
resolution. Source removal and fixture relocation alone do not satisfy it.

Consolidation removes eighteen verification derivations: fifteen public roots,
one nested module-validation derivation and the CA fixture/runner. Twenty-two
expression helpers and six fixture generators remain unique component inputs,
not gates. The measured recursive derivation union for core plus the five-package
slice decreases 5,428 → 4,432 (996 fewer). No runtime speedup is claimed without
measurement. Go dataset invocations decrease 9 → 3 and OpenSSL parses
1,758 → 879; all thirteen unchanged CA library tests run in normal core tests.

A broader dependency-recipe comparison includes the fifteen former wrappers
and seven ordinary component/database/documentation roots before consolidation,
versus their surviving roots plus all direct-CI tools and generated module
script contexts afterward. `nix derivation show --recursive` reports
7,563 → 7,547 unique recipes: **16 fewer overall in that comparison set**.
The 996 figure is only the narrower five-adapter slice, not the total pipeline.
Neither figure measures builds actually executed, cache hits, or elapsed time.
The unchanged core/embedded feature variants still test their distinct build
configurations; no Rust package tests were disabled.

The removed 94 normal-library selections were `cf-server` tests repeated by
the old DB wrapper. They are not the 94 `cf-config` tests now selected by core.
Ordinary config testing expands from the former standalone 13 CA tests to all
94 config tests (81 additional unit cases). Thus 102 redundant selections were
removed, but this is not a claim of 102 fewer total Rust test executions across
the entire CI pipeline, which also has feature variants and coverage measurement.

The ordinary core build passed (status 0 in
`/tmp/opencode/task490-config-core-staged.status`), including the retained Nix
protocol fragments, config library tests and CA comparison. Pure module tests,
installed-wrapper/config probes, actual writer XCCDF/OpenSCAP validation and
tool selftests also passed through the exact component CI commands.

The merged database workflow exposed runner-fixture defects, not waived
assertions: its worker config lacked required database fields, then inherited
SQLx-shell example settings changed `flakes.watched` into an invalid map.
The initial cleanup silently did nothing because Nix's Bash lacks `compgen`;
NUL-delimited environment enumeration now removes those development inputs.
The failing worker test subsequently passed (36.79 seconds), as did the 116
remaining exact tests and six broad groups (51 tests) in isolated follow-up
runs. The final canonical command then completed at source
`c254b1c54e0f40adfaa9d05b5766cfeffae9bb89`:
`nix run .#devScripts.state-machine-test -- -t=false up` exited **0**. Its 227
Rust result summaries total **515 passed, 0 failed, 0 ignored** across the
284 selected ignored-library tests and 231 selected integration cases, with
both populated migration rehearsals also passing. It ran from 20:39:03 to
20:57:29 UTC (18 minutes 26 seconds), then shut down its disposable PostgreSQL
cluster. Evidence is `/tmp/opencode/task490-db-final.log` and the status file
`/tmp/opencode/task490-db-final.status`. This is a lower-level result, not
Delivery/Assurance/Upgrade integration evidence. An invocation-owned child-status
receipt prevents process-compose success from hiding test failure: a local
exercise preserved child status 101 even with compose status 0, and rejected a
missing receipt. No product code was changed.

At clean `c254b1c54e0f40adfaa9d05b5766cfeffae9bb89`, the normal Access CI
command passed all 23 cases with a newly executed controller VM. Evidence:
`/nix/store/nv2rpc4szk7525aa9fpb4289nz1hpbqv-vm-test-run-cf-it-access-lifecycle-local-bootstrap-run`.
The original sixteen cases remain first and unchanged. Seven new cases prove
UI account creation (including first/last name, role and membership reads),
role edit, create/edit cancel, disable plus sign-in 401/no new session, retry
settings, banner enable and marking persistence. Each phase reloads in the
production UI and returns to independent SQL observations before the next
mutation. Screenshots/results use distinct browser artifact directories.
No environment-CRUD, outage-draft or visual-parity completion is claimed.

This remains one registered VM scenario. The successful VM driver took 50.72
seconds; startup/migrations took 10.5 seconds once and all later cases reused
that setup. There were two local VM runs while developing the form slice: the
first retained the original sixteen passes and four form passes before an
overly exact retry-field locator failed; the corrected final run passed all
23. A prior 300-second package build interruption started no VM. Production
package tests stayed enabled; the metadata-package input identity required a
one-time embedded-server rebuild. The controller is configured for 4 GiB/4
cores; peak memory remains unmeasured. No additional VM derivation was added.

## Source-retirement verification (historical milestone)

At clean source `0105e512baa80ace5f181918bbb72951bf506a50`, the exact
inventory evaluation returned `["integration-access"]` (27 attributes before
this pass; one afterward). This is not the final four-owner suite.

`sh packages/ci/integration-owner-build.sh integration-access` passed all
16 existing cases. The VM executed anew and recorded that source revision;
its driver script completed in 24.23 seconds. Evidence is retained at
`/nix/store/n68lcq6xxab1rhhfymp7yz4pj19hff95-vm-test-run-cf-it-access-lifecycle-local-bootstrap-run`
and exported beneath `integration-evidence/integration-access/`. Scenario and
browser assertions are unchanged. No new scenario was registered.

At that milestone, all 17 relocated package `drvPath`s evaluated. Lightweight contract/tool
packages, CA compatibility (13 selected Rust tests), OSCAL validation, and
builder module validation passed. The host browser runner selftest, design
generator tests, browser harness static contracts, and CI cache-policy/routing
test passed. GitLab CI lint accepted the new package matrix with no errors or
warnings; no deleted attribute remains in an active CI command.

The full `verify-server-regressions` build exceeded the 600-second local
command limit after starting isolated PostgreSQL, completing migration
rehearsals, and passing the 13 assignment-semantics tests and the zombie-repair
test. It is **not a full passing regression result**. Its definition and
derivation are identical to the former check, including all selected tests
and database hooks. The combined `verify-builder-evaluator-packaging` and
`verify-xccdf-schema` build also exceeded 600 seconds while compiling the
server dependency. Their full results remain unverified. No test was skipped
or changed to obtain a pass. These removed package commands are historical;
verify their consolidated successors with a longer build window or exact-head
CI evidence before review completion.

Documentation validation at that milestone ran as `nix build .#okf-knowledge -L`; it passed
after updating relocated links and the fixture README manifest entry. The
documentation-only follow-up does not require another Access VM run.

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

TASK-490.1 foundation is in `Review`; all seven bounded foundation criteria and documentation validation passed. At this revision the Operator/Viewer cases were not yet implemented; see [Access authorization cases](#access-authorization-cases-task-4902) for the later result. TASK-490.2 remains `In Progress`: exact cross-environment scope and closed-registration policy cases await their accepted decisions. These documentation-only follow-ups occur after the tested source and do not require another VM run.

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

### Collector failure handling (TASK-490.24)

The reviewed collector had two defects after a failed owner build. The Nix expression that lists the scenarios joins records with newlines and adds none after the last record, so the shell `read` loop skipped the last scenario; a single-scenario owner collected nothing. A failed copy under `set -e` also ended the script with the copy status instead of the build status. Both are corrected in `packages/ci/integration-owner-build.sh`. A temporary command-shim exercise against the actual script reproduced the raw listing format and ran 15 cases. The reviewed version passed 7 of them, failing the last-record cases and the copy-failure status cases. The corrected version passed all 15: one, three and mixed existing scenarios without a final newline; a real read-only store output; missing outputs; a failed evaluation; copy failures that keep status 17 or 23; and a successful build whose evidence is missing, absent or uncopyable, which still fails the job. The exercise is not committed as a test suite.

### Access authorization cases (TASK-490.2)

Tested source: `8a857bc872cf331a19036b97a93f2b7741e9728b`, on a clean tree. Command:

```bash
sh packages/ci/integration-owner-build.sh integration-access
```

The script exited 0. This run **executed the VM**: the source revision changed, Nix built 25 derivations including `vm-test-run-cf-it-access-lifecycle-local-bootstrap-run`, and the driver log shows `source 8a857bc8…`. The job took about 110 seconds; the recorded case time was 29.1 seconds. The scenario `result.json` reports `status: passed` and **16 of 16 cases**: the ten earlier cases unchanged and in order, followed by six new cases. No additional VM derivation, owner, or flake check was added. The scenario still registers once in `checks/integration-access/default.nix`.

| New case | What it proves |
| --- | --- |
| `authorization-actors-created-through-admin-api-and-signed-in` | The Admin API creates the Operator (design user `u2`), Viewer (`u4`) and a protected target (`u7`). Each persisted account matches the design identity, the generated username equals the authored local part, and each holds exactly its role. Each actor signs in through the real login, and `/api/auth/whoami` returns that actor's persisted id and role. |
| `authorization-admin-controls-accept-every-probe-payload` | Admin sends every probe payload first: retry policy 200, classification banner 200, escalation-by-create 201 then removal 204, escalation-by-PATCH 200 then restore 200. Each change is read back from the database and restored, so a later 403 can only come from the actor's role. |
| `authorization-operator-and-viewer-user-create-rejected` | Operator and Viewer each `POST /api/v1/admin/users` with the escalation payload: 403 `forbidden`, no account with that email, protected state unchanged. |
| `authorization-operator-and-viewer-user-patch-rejected` | Each `PATCH`es another account and its own account with `{role: Admin, enabled: false}`: 403 `forbidden` four times, no role or enablement change. |
| `authorization-operator-and-viewer-admin-configuration-rejected` | Each `PUT`s the automatic retry policy and the classification banner: 403 `forbidden` four times, values still the installed ones. |
| `authorization-viewer-has-no-usable-admin-mutation-in-ui` | An independent Viewer browser context signs in, finds no `/admin` navigation entry, sees the production "Access Denied" state at `/admin` (also after a reload), finds none of the Admin page's mutation buttons, and issues no non-GET admin request. Protected state is unchanged. |

Every denied request used valid session cookies and a matching CSRF header from the actor's own cookie jar. Before each probe the helper confirmed a live session, the persisted role and the `whoami` identity, so a 403 from a missing session cannot pass. The response check requires the documented `forbidden` error code, which also excludes the `csrf_validation_failed` response. The protected state compared before and after each probe covers users (identity, enablement, password digest), role assignments, environment memberships, the user-administration audit counts, the retry policy and the banner. Session rows and last-seen times are excluded on purpose. The baseline recorded 4 users, 4 role assignments and audit counts that match the Admin control sequence (`user_role_assigned` 6, `user_created` 4), which shows the comparison observes real changes.

Evidence: the retained scenario output is `/nix/store/xld57xqbfpvqpcmbqjij5djsm9l2j0mv-vm-test-run-cf-it-access-lifecycle-local-bootstrap-run`, and the owner aggregate is `/nix/store/4fqzi6xrlk5vz5w6shmlflvkcazi6hy9-integration-access`. The exported workspace directory held 1.2 MB of real files. `result.json` records, as receipts, the three created user ids, each of the ten denials (`authorization.denied.<actor>.<operation>`: method, status 403, error `forbidden`, `stateUnchanged: true`), the Admin control statuses, and the protected-state baseline. The Viewer browser result passed at `http://localhost:3000/admin`, and its screenshot shows the Viewer "Kit Thomas", a navigation without a Server entry, and the "Access Denied" section. The fresh-install Coach panel is open in that screenshot; the spec did not interact with it. No real runtime token appears in the build log or evidence; the only inline `x-csrf-token` value is the deliberately fake mismatched literal.

Reviewer artifacts: [scenario result](https://gitlab.com/-/project/70402481/uploads/678acd9a2c79d87d72d81c70cad2abd4/result.json), [Viewer browser result](https://gitlab.com/-/project/70402481/uploads/dc58018286300818fd347b58b862aea5/viewer-admin-denied-result.json), and [Viewer Access Denied screenshot](https://gitlab.com/-/project/70402481/uploads/e840b7c9cd5b2bfa1222deea8e788826/viewer-admin-access-denied.png).

Not covered by these cases: the cross-environment denial rule and the closed-registration policy (each awaits its own decision), an Operator browser view, other Admin routes (OIDC mappings, user DELETE as a non-Admin, heartbeat configuration), and any visual comparison. These cases make semantic assertions only and claim no visual parity. No product defect was found, and no production, UI, API, module, migration or schema file changed.

The GitLab-hosted `integration-access` job also passed all 16 cases in MR pipeline 2960613754 at `08a79037f872ec7743293a9d973a21f9aa4acebb`. The runner built 58 derivations, so the VM ran there; the job took about 74 seconds and uploaded a 1.0 MB evidence artifact. The earlier pipeline 2960470533 at `d28dfc9a` passed the same job with ten cases. The documentation and task-record commits after `08a79037` are not separately verified. That same earlier pipeline failed `flake-check: [server-regressions]` in `poam_workflows` with no Rust change (it passed at `9c6ce8ff`); this is recorded as the separate defect TASK-494 and is unrelated to these cases.

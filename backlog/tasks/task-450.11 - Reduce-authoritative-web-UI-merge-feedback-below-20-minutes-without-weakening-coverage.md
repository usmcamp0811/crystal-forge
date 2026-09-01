---
id: TASK-450.11
title: >-
  Reduce authoritative web UI merge feedback below 20 minutes without weakening
  coverage
status: In Progress
assignee:
  - opencode-gpt-5.6-sol
created_date: '2026-09-01 03:12'
updated_date: '2026-09-01 17:34'
labels:
  - web-ui
  - testing
  - nix
  - playwright
  - ci-performance
dependencies:
  - TASK-438
references:
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/pipelines/2807718751'
  - TASK-354
  - TASK-438
parent_task_id: TASK-450
priority: high
type: enhancement
ordinal: 11000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

The authoritative `web-ui` merge-request check remains the longest blocking job. Pipeline 2807718751 was still running after 21 minutes even after the TASK-450 P0 build-graph changes. The check combines the embedded production server, the Dioxus WASM build, a large sequential Playwright workflow, screenshot capture, visual design-parity processing, and OSCAL and SARIF export validation.

Long feedback delays iteration and makes the web UI job the merge-request critical path.

## Goal

Return a trustworthy, merge-blocking web UI verdict in less than 20 minutes while preserving the existing production and coverage guarantees.

## Required guarantees

- A blocking pre-merge check must still exercise the production server binary serving the embedded production WASM through a real browser.
- Required browser-step failures must fail the merge gate and retain useful diagnostic artifacts.
- Runtime improvements must not remove semantic coverage, screenshot evidence, export validation, or design-parity evidence. Evidence that does not determine the merge verdict may complete in parallel.
- The result must remain reproducible in the repository Nix environment.

## Coordination

TASK-438 is the correctness prerequisite because runtime measurements are not meaningful while failed required browser steps can be reported as success. TASK-354 tracks existing Playwright flakiness and deterministic fixture issues that can otherwise prevent safe concurrency or tighter timing bounds.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Phase-level timing records distinguish Nix build or substitution time, VM startup and fixture setup, required Playwright execution, export validation, design-parity processing, and artifact publication
- [ ] #2 At least three representative merge-request pipeline runs produce the required blocking web UI verdict in less than 20 minutes of job execution time, with the median and maximum durations recorded
- [ ] #3 A blocking pre-merge check exercises the production server binary serving the embedded production WASM through a real browser
- [ ] #4 Every required selected browser-step failure causes the merge gate to fail and preserves the failed step name, reason, and available diagnostic artifacts
- [ ] #5 Existing required semantic browser coverage, screenshot evidence, OSCAL validation, SARIF validation, and design-parity evidence remain available after the runtime change
- [ ] #6 Runtime reduction does not depend on suppressing failures, unbounded retries, or shorter timeouts that make supported CI runners unreliable
- [ ] #7 The repository documents the authoritative web UI checks, their responsibilities, and the local Nix commands used to reproduce each check
- [ ] #8 The production OSCAL Assessment Results download passes the vendored NIST OSCAL 1.1.2 schema in the isolated blocking browser export check
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Land browser-result correctness and deterministic focused workflows first.
2. Refactor the existing Web UI NixOS test into one shared parameterized evidence constructor and a blocking verdict wrapper.
3. Expose a small check set: production embedded-server smoke/shell coverage, balanced required semantic groups, independent browser export validation, and advisory design-parity evidence.
4. Schedule blocking checks concurrently in GitLab. Preserve unique evidence paths and aggregate all statuses and artifacts into one reviewer-facing report.
5. Document the logical gate and exact local commands. Run local focused checks, then use representative MR pipelines to balance groups and demonstrate a blocking critical path below 20 minutes without reducing coverage.

Post-fleet correction: encode required versus advisory ownership for every ci_fast step while preserving exactly-once shard execution and compatibility smoke's six required steps. Extend browser and aggregate verdicts so only required failures block, while advisory failures and missing advisory results remain visible. Fix route teardown across failed steps/page recreation, make 12f and 12h focused fixtures self-contained and condition-synchronized, preserve context-scoped coach suppression, update gate documentation, and run focused/static/Nix verification without rerunning the full fleet unless needed.

User-approved scope expansion: correct the production OSCAL Assessment Results generator so the real browser download passes the vendored NIST OSCAL 1.1.2 schema. Limit the production change to the three observed shape defects, add focused generator regression assertions, rerun the isolated blocking exports VM check, then continue the existing final verification and MR plan.

Final review correction: make generated OSCAL control IDs NCName-safe and stable, cover names with punctuation, keep empty-scope exports schema-valid or explicitly constrained, and correct the documented AP/SSP reference guarantee. Add producer cache-state and queue metadata plus aggregate per-job timing, median, maximum, and blocking critical-path reporting before representative MR runs. Run the aggregate report through a flake-pinned Node/curl package instead of a mutable Alpine image. Correct stale baseline and SARIF fixture prose, remove local result links, then rerun focused Rust/schema and static CI/Nix verification.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
The user approved a multiple-check topology so GitLab can schedule Web UI work concurrently across available runners. Treat the complete blocking set as one logical merge gate. The intended responsibility split is: a small production smoke check for the embedded server and production WASM in a real browser; independently reproducible required E2E shards with isolated deterministic state; separate OSCAL/SARIF browser export validation; and separate advisory design-parity evidence. Start with a small shard count and measure VM startup and runner overhead before adding more. Shared Nix inputs must retain identical derivation paths so parallel checks reuse build outputs rather than recompile them. CI artifacts must use collision-free per-check paths and one aggregate reviewer-facing report.

The user selected the umbrella and its three subtasks for one shared branch, worktree, and MR together with prerequisite TASK-438 and flakiness task TASK-354. TASK-430, TASK-450.8, and TASK-450.10 remain outside this focused MR.

LOCK: opencode-gpt-5.6-sol in /home/mcamp/code/crystal-forge/TASK-450-web-ui-parallel-checks on branch TASK-450.11-web-ui-parallel-checks, based on TASK-450-p0-build-graph at 437efd55.

Applied focused post-audit fixes in the existing TASK-450.11 worktree. Browser coach suppression now uses BrowserContext init scripts for non-onboarding selections; compatibility smoke retains ordered auth, dashboard, embedded assets, and Chromium with duplicated shell steps removed. Advisory design commands no longer swallow statuses and record command failures/missing outputs in check-verdict.json; producer metadata classifies false advisory verdicts, evidence lookup failures, and evidence copy failures. Added VM phase timings, producer gate/transfer timings, aggregate timing metadata, producer-specific required evidence, bounded inline uploads, aggregate evidence retention, masked GITLAB_TOKEN PRIVATE-TOKEN publication behavior, exact reusable gate checker regression fixtures, export-only semantic skip, and corrected runbook baseline/CI claims.

Verification completed without VM execution: Nix dev-shell Node syntax and 20 Node tests passed; ownership validator reported 100 ci_fast steps partitioned across fleet/pipeline/governance; bash -n and nixpkgs shellcheck passed; all six Nix files parsed and all six path:. check names evaluated; .gitlab-ci.yml parsed with nixpkgs yq-go; git diff --check passed. No commit or push was made.

Post-fleet correction implemented without commit/push. `check-groups.json` now classifies all 100 ci_fast steps exactly once as 17 required and 83 advisory; the historical 15-step required set is preserved and registration-submit/post-register-login are required auth prerequisites. Browser schema v2, Nix check verdicts, the exact gate checker, and the CI aggregate report expose required versus advisory failures; missing advisory results remain reported but do not block. Failed-page teardown now waits for route handlers before page recreation and reinstalls page-scoped standalone/onboarding fixtures while setup-coach suppression remains BrowserContext-scoped.

Focused VM evidence command `CF_UI_TEST_STEPS="12f-systems-deploy-modal,12h-system-detail-cves-grouped-justification" nix build --impure path:.#checks.x86_64-linux.web-ui-fleet.evidence -L --no-link` completed successfully in 59.26 seconds. It produced schema-v2 browser/check verdicts with `ok: true`, no required failures, both selected failures under `failedAdvisorySteps`, and `processError: null`; the prior detached `Route is already handled` rejection did not recur. 12f and 12h remain product-contract blockers rather than test flakiness: current System Detail Deploy only selects an inline Deploy tab and never opens `DeploySystemModal`; current CVE component explicitly groups package-first and omits the asserted CVE-first filters. 12h retained `role=tab` and now records decisive surface text diagnostics. Assertions were not weakened.

Final verification passed: 27/27 Node tests; static ownership validator (`100 ci_fast`, `17 required`, `83 advisory`); JSON parse; Node syntax for all 14 Web UI/CI JS files; Nix parse for six Web UI check files; all six check names evaluated earlier; final generated NixOS test driver built and its Python `test-script` compiled; `bash -n`; ShellCheck 0.11.0; `.gitlab-ci.yml` yq parse; exact checker against focused advisory evidence exited 0 while printing both advisory failures; `git diff --check`. Full fleet was not rerun. Historical remaining advisory failures are 12d2, 12e, 12f, 12k, 12h, 12g, 16c, and 28; only 12f/12h were focused browser-verified in this pass.

Full `web-ui-fleet` validation now completes with the final timeout and route-recovery changes. `nix build path:.#checks.x86_64-linux.web-ui-fleet -L --no-link` passed. Evidence `/nix/store/j3sygsbqlzvy2l6x8l2dymgsq24j7i94-vm-test-run-crystal-forge-web-ui-fleet-evidence` records a 139.873-second total VM evidence duration and 120.822-second browser semantic duration. Its blocking check verdict is `ok: true`; all required steps passed; OSCAL and SARIF component verdicts are true; `processError` is null. Six advisory failures remain explicit: 12d2, 12e, 12k, 12g, 16c, and 28. Updated 12f and 12h contracts both passed.

Final static verification passed: Node syntax, 28/28 browser/group/CI tests, Nix parse for `checks/web-ui/default.nix`, and `git diff --check`. This local result does not satisfy acceptance criterion 2, which still requires three representative merge-request pipeline runs below 20 minutes. No commit or push was made.

Remaining exports VM verification exposed a production OSCAL 1.1.2 defect after the isolated harness was updated to current local-auth, compliance routing, Import / Export menu, and setup-coach behavior. The real OSCAL download fails the vendored NIST schema because `assessment-results.local-definitions.components` is not allowed, `objectives-and-methods` has the wrong shape, and `reviewed-controls.control-selections` is missing. The same run's real SARIF download passes schema and semantic validation. No existing dedicated OSCAL schema bug task was found; TASK-318 is a broader Backlog feature. This scope decision requires user approval before changing production export generation or weakening the new blocking gate.

Final review corrections are implemented and locally verified. OSCAL control IDs now use stable NCName-safe policy UUID tokens. Control selections enumerate the selected policy controls and represent an empty scope explicitly. The generator omits optional empty arrays, emits mandatory SSP sentinels for an empty scope, maps all control statuses to valid OSCAL target states, and omits empty `relevant-evidence`. The browser export fixture covers punctuation, `NotChecked`, `NotApplicable`, controls without evidence, and a second empty-scope download.

Final `nix build path:.#checks.x86_64-linux.web-ui-exports -L --no-link` passed. The VM recorded 35.288 seconds total and 15.946 seconds in export validation. Populated and empty-scope Assessment Results, embedded Assessment Plans, and embedded SSPs all passed vendored NIST OSCAL 1.1.2 schemas and reference-chain checks. SARIF schema and semantic validation also passed.

CI producer metadata now records bounded GitLab Jobs API runner queue duration, local/substituted/built/mixed cache state, Nix realization time, evidence lookup, and evidence copy. Aggregation reports every job duration and only computes median, maximum, and blocking critical path when all five blocking producer timings are present. Producer and aggregate scripts run through flake-pinned Nix packages; the mutable Alpine report image was removed.

Final static verification passed: 28/28 Node tests; ownership validation (100 ci_fast, 17 required, 83 advisory); JavaScript syntax; `bash -n`; ShellCheck 0.11.0; GitLab YAML parse with yq-go; targeted Rust OSCAL test; rustfmt check; both helper package builds; all six check attribute evaluations; helper package evaluations; and `git diff --check`. The three representative MR pipelines required by acceptance criterion 2 remain pending until the branch is committed, pushed, and an MR exists.
<!-- SECTION:NOTES:END -->

---
id: TASK-354
title: Investigate web-ui check flakiness and consistent 12f/12h system test failures
status: In Progress
assignee:
  - opencode-gpt-5.6-sol
created_date: '2026-06-13 19:35'
updated_date: '2026-09-01 18:07'
labels:
  - web-ui
  - testing
  - flaky-test
  - tech-debt
dependencies: []
references:
  - TASK-450.11
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/325'
modified_files:
  - checks/web-ui/tests/integration-test.js
  - checks/web-ui/tests/check-groups.test.js
  - checks/web-ui/coverage-manifest.json
  - checks/web-ui/check-groups.json
  - checks/web-ui/default.nix
  - docs/web-ui-check.md
priority: medium
ordinal: 299000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

While running `nix build .#checks.x86_64-linux.web-ui` during TASK-353, the VM-based web-ui integration check exhibited two distinct issues discovered out-of-scope:

1. **Widespread flakiness**: Across repeated runs, a shifting set of modal-open/interaction tests fail with `locator.click: Timeout 30000ms` or "modal did not open" errors. The failing set changes between runs and spans unrelated surfaces (06-dashboard-loading-spinner, 09i-topbar-notifications-non-admin, 11c-builders-edit-modal, 13e/13f/13g-flakes-*, 13h-flakes-force-push). This points to VM resource contention / timing rather than product bugs. A contributing factor: an absolutely-positioned "Setup Coach" onboarding overlay intercepts pointer clicks on the system detail header actions (confirmed visually in the 12k screenshot), which can break `.click()` without `force: true`.

2. **Consistently failing tests** (independent of TASK-353 changes, in code paths TASK-353 did not modify):
   - `12f-systems-deploy-modal` — "Expected Deploy modal to POST a deploy request" (sometimes "Expected deploy modal heading to be visible"). Uses the systems-list DeploySystemModal + `.sd-commit-item` selection.
   - `12h-system-detail-cves-grouped-justification` — `locator.click` timeout. Uses `getByRole("button", { name: "CVEs" })` to click the CVEs tab, but the tab rail buttons have `role="tab"` (not `button`), so this selector does not match the tab element. This selector mismatch predates TASK-353 (the tab rail used `role="tab"` on `dev`).

Note: The web-ui VM derivation currently exits 0 even when individual Playwright steps fail (the harness logs failures but does not fail the build). This masks regressions and should be reviewed.

## Desired Outcome

- The web-ui check reliably passes (or deterministically reports real failures) without environment-driven flakiness.
- `12h` clicks the CVEs tab via a selector that matches `role="tab"` (or the tab gets an appropriate accessible role).
- `12f` deploy-modal POST assertion passes deterministically.
- Decide whether the "Setup Coach" overlay should be suppressed/dismissed in the test harness so header-action clicks are not intercepted.
- Consider making the VM test derivation fail when Playwright steps fail, so regressions are not masked.

## Notes / Evidence

- Discovered during TASK-353 parity work (MR !275).
- TASK-353 fixed its own `12e` regression and added a deterministic `12k` test (8-tab rail + Compliance + header actions) that passes and captures a screenshot.
- The interception overlay is visible in `12k-system-detail-tab-icons.png`.
<!-- SECTION:DESCRIPTION:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Add atomic current-step publication immediately before each browser action and include that record in fatal diagnostics.
2. Wrap the Node integration process in GNU `timeout` with TERM plus a bounded KILL grace period. Atomically publish `integration.exit` for every process outcome. Make the Nix driver classify timeout exit codes as infrastructure failure, print integration log, current step, and server journal, and refuse to consume or synthesize a logical verdict.
3. Add static regression coverage for current-step publication, route cleanup, and timeout diagnostics. Document the final shard timeout rationale against the less-than-20-minute gate target.
4. Temporarily use a shorter process timeout to reproduce the full fleet stall with diagnostics. Identify the exact step and repair its route, promise, or cleanup lifecycle without removing any ci_fast step.
5. Restore the justified final timeout. Run Node/static checks, focused 12f/12h evidence if affected, and full `web-ui-fleet` evidence until all required steps pass, `processError` is null, and the gate succeeds. Record total runtime and advisory failures.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
TASK-450.11 sets a less-than-20-minute blocking web UI feedback target. Resolve or characterize the fixture, selector, overlay, resource-contention, and timeout failures here so later sharding or concurrency does not amplify flakiness. Record whether each failure is product behavior, deterministic harness drift, or CI resource sensitivity. Runtime improvements must not rely on suppressing these failures or reducing timeouts below reliable bounds.

The user selected this task for the focused Web UI latency bundle and explicitly requested one shared branch, worktree, and MR with TASK-438 and TASK-450.11.1 through TASK-450.11.3.

LOCK: opencode-gpt-5.6-sol in /home/mcamp/code/crystal-forge/TASK-450-web-ui-parallel-checks on branch TASK-450.11-web-ui-parallel-checks, based on TASK-450-p0-build-graph at 437efd55.

Implemented the requested harness fixes in the shared TASK-450 worktree. Non-onboarding profiles/focused runs now suppress the setup coach, full onboarding keeps its route-backed walkthrough and cleans routes/storage when leaving the onboarding block, and focused onboarding installs its own fixture. Step 12h now selects `role=tab` and cleans every route in `finally`. Step 12f now waits for modal state, selected commit state, POST request, and successful response instead of a delay. OSCAL/SARIF catches now record the interrupted step so prior successes cannot hide partial failure. Targeted Node syntax checks passed; browser/VM execution was not run in this focused harness pass.

Implemented the approved current-product contracts in the shared TASK-450.11 worktree without commit or push. Renamed 12f to `12f-system-detail-inline-deploy`; it now uses accessible header/tab/commit/action locators, verifies selected-tab and selected-commit state, validates the exact deploy POST payload and successful response, and asserts the inline success callout. Renamed 12h to `12h-system-detail-cves-package-workflow`; it retains `role=tab`, verifies package-first expansion as detail filtering, CVSS and NVD detail behavior, justification preset and exact PUT payload, save acknowledgement, resource refresh, and the accessible `justified` state. Updated coverage manifest, real-trigger exclusion text, fleet advisory membership, and the route-cleanup/tab static test. Both remain advisory and exactly-once owned. No docs referenced the old names.

Final verification passed. `nix develop path:. -c node --check checks/web-ui/tests/integration-test.js`; 27/27 Node tests via `nix develop path:. -c node --test checks/web-ui/tests/browser-verdict.test.js checks/web-ui/tests/check-groups.test.js ci/web-ui-ci.test.js`; ownership validator reported 100 ci_fast steps, 17 required and 83 advisory; `git diff --check` passed. Focused real VM command `CF_UI_TEST_STEPS="12f-system-detail-inline-deploy,12h-system-detail-cves-package-workflow" nix build --impure path:.#checks.x86_64-linux.web-ui-fleet.evidence -L --no-link` passed both selected steps and captured four themed screenshots. Evidence `/nix/store/8158xgxj8zcqskxllavw2v5vs8rqczlh-vm-test-run-crystal-forge-web-ui-fleet-evidence/screenshots/verdict.json` records `completed: true`, `ok: true`, each selected step `ok: true` with `reason: null`, no failed required/advisory steps, and `processError: null`. Browser semantic execution was 5.693 seconds; total VM evidence duration was 23.151 seconds.

Full default-timeout fleet verification completed after the route-lifecycle fixes. Root cause: delayed dashboard route handlers could call `route.fulfill()` after route removal, producing `Route is already handled!`; when advisory step `16c-scanning-view` then failed, sequential `page.unrouteAll()` and `page.close()` recovery could wait indefinitely. Delayed fulfills now tolerate canceled or already-handled requests, and failed-page cleanup runs `page.unrouteAll({ behavior: "ignoreErrors" })` and `page.close()` concurrently with `Promise.allSettled`.

`nix build path:.#checks.x86_64-linux.web-ui-fleet -L --no-link` and `nix build path:.#checks.x86_64-linux.web-ui-fleet.evidence --no-link --print-out-paths` passed with the default 900-second process timeout. Evidence: `/nix/store/j3sygsbqlzvy2l6x8l2dymgsq24j7i94-vm-test-run-crystal-forge-web-ui-fleet-evidence`. The browser verdict is complete and `ok: true`; all required steps passed; `12f-system-detail-inline-deploy` and `12h-system-detail-cves-package-workflow` passed; `processError` is null. Six advisory failures remain visible: 12d2, 12e, 12k, 12g, 16c, and 28. Total VM evidence duration was 139.873 seconds; browser semantic execution was 120.822 seconds.

Final static verification passed: Node syntax, 28/28 browser/group/CI tests, `nix-instantiate --parse checks/web-ui/default.nix`, and `git diff --check`. No commit or push was made.
<!-- SECTION:NOTES:END -->

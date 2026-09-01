---
id: TASK-354
title: Investigate web-ui check flakiness and consistent 12f/12h system test failures
status: In Progress
assignee:
  - opencode-gpt-5.6-sol
created_date: '2026-06-13 19:35'
updated_date: '2026-09-01 03:59'
labels:
  - web-ui
  - testing
  - flaky-test
  - tech-debt
dependencies: []
references:
  - TASK-450.11
modified_files:
  - checks/web-ui/tests/integration-test.js
  - checks/web-ui/tests/oscal-export-test.js
  - checks/web-ui/tests/sarif-export-test.js
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
1. Reproduce and classify the setup-coach leak, the `12h` accessible-role mismatch, and the `12f` deploy-modal timing path in focused execution.
2. Keep the onboarding walkthrough isolated from ordinary check groups. Ensure non-onboarding groups begin with the setup coach suppressed and do not inherit onboarding routes or overlay state.
3. Correct `12h` to use the application tab role and make route cleanup reliable after failure.
4. Replace timing-sensitive `12f` actions with observable modal, request, and response conditions while preserving the product behavior under test.
5. Run the focused workflows repeatedly in their final group and record whether remaining failures are product defects, deterministic harness defects, or runner resource sensitivity.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
TASK-450.11 sets a less-than-20-minute blocking web UI feedback target. Resolve or characterize the fixture, selector, overlay, resource-contention, and timeout failures here so later sharding or concurrency does not amplify flakiness. Record whether each failure is product behavior, deterministic harness drift, or CI resource sensitivity. Runtime improvements must not rely on suppressing these failures or reducing timeouts below reliable bounds.

The user selected this task for the focused Web UI latency bundle and explicitly requested one shared branch, worktree, and MR with TASK-438 and TASK-450.11.1 through TASK-450.11.3.

LOCK: opencode-gpt-5.6-sol in /home/mcamp/code/crystal-forge/TASK-450-web-ui-parallel-checks on branch TASK-450.11-web-ui-parallel-checks, based on TASK-450-p0-build-graph at 437efd55.

Implemented the requested harness fixes in the shared TASK-450 worktree. Non-onboarding profiles/focused runs now suppress the setup coach, full onboarding keeps its route-backed walkthrough and cleans routes/storage when leaving the onboarding block, and focused onboarding installs its own fixture. Step 12h now selects `role=tab` and cleans every route in `finally`. Step 12f now waits for modal state, selected commit state, POST request, and successful response instead of a delay. OSCAL/SARIF catches now record the interrupted step so prior successes cannot hide partial failure. Targeted Node syntax checks passed; browser/VM execution was not run in this focused harness pass.
<!-- SECTION:NOTES:END -->

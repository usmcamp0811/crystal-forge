---
id: TASK-354
title: Investigate web-ui check flakiness and consistent 12f/12h system test failures
status: Backlog
assignee: []
created_date: '2026-06-13 19:35'
labels:
  - web-ui
  - testing
  - flaky-test
  - tech-debt
dependencies: []
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

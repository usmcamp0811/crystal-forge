---
id: TASK-MEDIUM.3
title: Add UI test automation for web UI checks
status: Done
assignee: []
created_date: '2026-02-14 04:41'
updated_date: '2026-02-14 05:16'
labels:
  - ui
  - testing
  - ci
dependencies: []
parent_task_id: TASK-MEDIUM
milestone: m-3
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Introduce a UI testing framework (evaluate Playwright vs Selenium vs Cypress) to drive the web UI checks, including toggling Systems table/cards view during screenshots. Ensure the NixOS VM test can run the tool headlessly and capture screenshots deterministically.
<!-- SECTION:DESCRIPTION:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented Playwright-based UI test automation in checks/web-ui/default.nix:

- Uses pkgs.playwright-test and pkgs.playwright-driver.browsers for headless Chromium
- Inline JavaScript screenshot script runs inside the NixOS VM
- Captures 7 screenshots: dashboard, systems-table, systems-cards, builds, cves, style-guide, not-found
- For systems-cards, clicks the 'Cards' toggle button before capture
- Added data-testid attributes to systems_list.rs for reliable element selection
- Results written to JSON and copied to $out/screenshots/

All 7 screenshots captured successfully in ~15 seconds.
<!-- SECTION:NOTES:END -->

---
id: TASK-40
title: Expand web-ui check screenshots for new views and modals
status: Done
assignee: []
created_date: '2026-02-17 04:23'
updated_date: '2026-02-17 04:23'
labels:
  - web-ui
  - checks
  - testing
dependencies: []
priority: high
milestone: m-3
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Update checks/web-ui screenshot capture script to include newly added environments/flakes/systems UI states and modal dialogs, and verify artifacts are produced by nix web-ui check.
<!-- SECTION:DESCRIPTION:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Extended checks/web-ui/default.nix Playwright route list to capture screenshots for new Flakes and Environments views plus modal states (add/edit/remove/policy picker) and Systems modal states (add/keypair/remove). Added per-route setup hook support for opening modal states prior to capture. Verified with nix build .#checks.x86_64-linux.web-ui and confirmed screenshot artifacts in result/screenshots.
<!-- SECTION:NOTES:END -->

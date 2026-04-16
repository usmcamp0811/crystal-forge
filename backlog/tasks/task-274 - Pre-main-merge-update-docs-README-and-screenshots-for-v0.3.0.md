---
id: TASK-274
title: 'Pre-main-merge: update docs, README, and screenshots for v0.3.0'
status: To Do
assignee: []
created_date: '2026-04-16 23:53'
labels:
  - docs
  - screenshots
  - release
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

Before merging dev into main for v0.3.0, several docs and screenshots are stale or missing.

## Items to fix

### README.md
- Roadmap row v0.3.0: change "In Progress" → "Done"
- Add Evaluations view to the Web UI Views table (with screenshot link)
- Update feature description to mention eval cancellation and eval history view
- Fix screenshot references: README points to old numbered files (08/09/10/11/12) but test now generates differently-numbered files

### docs/context.md
- Update from v0.1.0 / 247 commits stale content to reflect v0.3.0 current state

### docs/specs/01-frontend-views.md
- Add /evaluations route to route mapping
- Add Evaluations view section documenting the Active Queue tab and History tab

### integration-test.js — new screenshot steps needed
- 26-evaluations: Evaluations page, Active Queue tab (with mocked eval queue data showing pending/in_progress items with Cancel buttons)
- 26b-evaluations-history: Evaluations page, History tab (with mocked history data showing complete/failed/cancelled items with duration)

### docs/screenshots — README-facing screenshots
- README.md currently links to 08-systems, 09-flakes, 10-environments, 11-builds, 12-cves
- These need to either be regenerated (via nix flake check) or the README updated to match the current test-generated names
- The README dashboard screenshot (06-dashboard.png) is Mar 16 — check if current

## Acceptance Criteria
- [ ] README v0.3.0 roadmap row shows "Done"
- [ ] README Web UI Views table includes Evaluations row
- [ ] docs/context.md reflects v0.3.0 current state
- [ ] docs/specs/01-frontend-views.md includes /evaluations route and Evaluations view section
- [ ] integration-test.js has at least 26-evaluations and 26b-evaluations-history screenshot steps
- [ ] All new screenshot steps added to CI_FAST_STEP_NAMES if they assert TASK-273 features
- [ ] `SQLX_OFFLINE=true cargo check` still passes after any Rust-touching changes (none expected)

## Verification
- Tier 0: `SQLX_OFFLINE=true nix develop -c cargo check` (no Rust changes expected)
- Manual: verify the new screenshot steps run in the integration-test.js test runner locally with node
<!-- SECTION:DESCRIPTION:END -->

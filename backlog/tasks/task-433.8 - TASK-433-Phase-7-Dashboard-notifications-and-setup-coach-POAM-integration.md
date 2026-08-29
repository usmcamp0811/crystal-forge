---
id: TASK-433.8
title: 'TASK-433 Phase 7: Dashboard, notifications, and setup-coach POA&M integration'
status: In Progress
assignee:
  - '@opencode-agent'
created_date: '2026-08-23 01:43'
updated_date: '2026-08-29 02:42'
labels:
  - design-parity
  - poam
  - web-ui
  - dashboard
  - phase-7
dependencies:
  - TASK-433.7
references:
  - TASK-433
  - TASK-433.1
documentation:
  - docs/design/CrystalForge/components/DashboardView.jsx
  - docs/design/CrystalForge/components/SetupCoach.jsx
  - docs/design/CrystalForge/data-dashboard.js
parent_task_id: TASK-433
priority: high
type: feature
ordinal: 440000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Objective
Parent umbrella: TASK-433, phase 7 of 8 (contextual only). Extends the existing notification and six-step setup-wizard conventions to POA&M, and wires the dashboard POA&M Summary/Watchlist to the Phase 5 API.

## Explicit scope
- Dashboard: real POAM Summary/Watchlist widgets using real batched APIs, preserving existing layouts and opening detail views (layout migration only where required).
- Notifications: real deduplicated overdue/awaiting-verification POAM events with target/read/dismiss/navigation and no render/poll spam.
- Setup coach: production-derived policy, bundle and Track-a-POAM steps added without breaking existing coach progress state.

## Explicit non-scope
No POA&M API changes (Phase 5 owns the API). No localStorage/`window.__cfCoach`/`CustomEvent` state; use existing coach/notification persistence conventions.

## Verification
```bash
nix develop -c cargo fmt --manifest-path packages/web-ui/Cargo.toml -- --check
nix develop -c cargo test --manifest-path packages/web-ui/Cargo.toml
nix build .#packages.x86_64-linux.web-ui --no-link
nix build .#checks.x86_64-linux.web-ui --no-link
```
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Dashboard POAM Summary and Watchlist use real batched APIs, preserve existing layouts and open detail.
- [ ] #2 POAM notifications are real deduplicated events with target/read/dismiss/navigation and no render/poll spam.
- [ ] #3 Setup coach adds production-derived policy, bundle and Track a POAM steps without breaking existing progress.
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Phase-7 preflight passed at branch head `7d6eef77be055f05ead86c7ac2eecc1d44ff1b18`: local and remote branch heads match, the worktree is clean, MR !318 is conflict-free, and TASK-433.7 is Review with AC1-AC4 checked. The authoritative Phase-6 Web UI check passed at implementation head `ff183d032a8dcc4794acf7064eb7acd8e61cdcb6`; `7d6eef77` is bookkeeping only. Phase-7 scope is dashboard, durable notifications, and production-derived setup coach steps. TASK-433.9 and final Phase-8 parity work remain prohibited.
<!-- SECTION:NOTES:END -->

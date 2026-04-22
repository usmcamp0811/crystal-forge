---
id: TASK-276
title: Add systemd hardening scanner and dashboard view
status: In Progress
assignee: []
created_date: '2026-04-19 02:43'
updated_date: '2026-04-22 12:38'
labels:
  - feature
  - security
  - systemd
  - dashboard
  - nixos
milestone: Security Visibility
dependencies: []
references:
  - >-
    https://www.reddit.com/r/homelab/comments/1spgay2/is_anyone_else_a_stickler_for_systemd_hardening/
  - 'https://www.freedesktop.org/software/systemd/man/systemd.exec.html#Security'
priority: high
ordinal: 3690
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Create a feature to scan NixOS system configurations for systemd service hardening options and display them in a dashboard view.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Static analysis successfully extracts systemd service configurations via nix eval from NixOS flake outputs
- [ ] #2 Database migrations create hardening_scans, service_hardening_results, and hardening_scan_justifications tables
- [ ] #3 Hardening score (0-100) calculated for each service based on presence/absence of 10-15 key security directives
- [ ] #4 Fleet-level dashboard displays overall hardening posture with score distribution and top 10 vulnerable services
- [ ] #5 System-level view shows per-service hardening breakdown with sortable/filterable table
- [ ] #6 Service detail view displays specific enabled/disabled directives and missing critical options
- [ ] #7 Scan can be triggered on-demand via admin endpoint for a specific system/commit
- [ ] #8 Automatic hardening scan triggers when new commits are processed
- [ ] #9 Justification system allows marking service findings as acceptable with reason text
- [ ] #10 Results integrate with existing system detail views (view_system_detail extended)
- [ ] #11 Color-coded risk indicators (green/yellow/orange/red) based on hardening score ranges
- [ ] #12 Scoring algorithm validated against Crystal Forge's own NixOS configurations
- [ ] #13 Hardening scan is integrated into the automatic evaluation pipeline (runs during dry-run eval)
- [ ] #14 Hardening data is available by default when a system is evaluated (no manual scan button needed)
- [ ] #15 Evaluation pipeline combines hardening scan logic with existing nix-eval-jobs to minimize overhead
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1) Update widget grid shell layout to ensure each widget card and content region uses flex + min-h-0 constraints so nested scroll containers work in all widget sizes.
2) Introduce dashboard-specific hardening renderers:
   - Top vulnerable services: width-constrained fixed table with truncation, count/score/range columns that do not overflow.
   - Environment posture: compact risk-first rows with score, counts, and concise watchlist links suitable for 2x3 dashboard widget.
3) Wire dashboard to new compact renderers while keeping full hardening page behavior intact.
4) Run targeted web-ui compile/tests and web-ui check build to verify no regressions.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
2026-04-22: addressed MR blocker feedback with focused backend fixes. (1) Hardened `GET /api/v1/hardening-scans/{scan_id}` auth: endpoint now resolves caller role + memberships and enforces environment-scoped access for non-admins via scan→derivation→system mapping, returning not_found when inaccessible. (2) Corrected manual scan target resolution to use the system’s current deployed store path (latest `system_states`) matched to derivation store/expected store path, instead of newest flake commit for config name. (3) Fixed nullable justification uniqueness/upsert semantics by adding migration 0116: dedupe existing NULL-directive rows, drop old nullable unique constraint, add partial unique indexes for service-level NULL and directive-level non-NULL cases; updated upsert query logic to use matching conflict targets and return canonical row ID. (4) Hardened scan execution terminal-state handling: post-scan DB persistence failures now call `mark_scan_failed` so scans do not remain in_progress on insert/complete errors. Verification: `nix develop -c cargo check` in `packages/web-ui` passed; `nix build .#checks.x86_64-linux.web-ui --print-out-paths` passed. Backend `nix develop -c cargo check` in `packages/default` is still blocked in this environment due sqlx compile-time DB connection refused (local DB not reachable from command context).
<!-- SECTION:NOTES:END -->

## Definition of Done
<!-- DOD:BEGIN -->
- [ ] #1 All database migrations pass and sqlx metadata is in sync
- [ ] #2 nix flake check passes
- [ ] #3 cargo fmt -- --check passes
- [ ] #4 cargo clippy -- -D warnings passes
- [ ] #5 Unit tests cover scoring algorithm logic
- [ ] #6 Integration tests verify scan trigger endpoints
- [ ] #7 Dashboard views render correctly with test data
- [ ] #8 No regressions in existing CVE scanning functionality
<!-- DOD:END -->

---
id: TASK-276
title: Add systemd hardening scanner and dashboard view
status: Review
assignee: []
created_date: '2026-04-19 02:43'
updated_date: '2026-04-19 16:15'
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
ordinal: 0
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
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Follow-up commit 63ce519b redesigns system-detail Hardening tab into dense risk-ranked audit dashboard layout with grouped hardening dimensions and compact status cells.

Added top summary rollups and lightweight filters/sort controls (search, severity, risky-only toggle, sort mode) using existing hardening API payload fields.

Updated deterministic web-ui integration step `28-system-hardening-tab` mocks/assertions for grouped headers + dense status cell evidence.

Verification pass after redesign: `nix develop -c cargo check` (packages/web-ui), `nix build .#checks.x86_64-linux.web-ui --print-out-paths`, and `nix flake check`.

Refreshed screenshot uploads for MR !242: `27-hardening-fleet` and `28-system-hardening-tab`; evidence posted in MR note https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/242#note_3264569822
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

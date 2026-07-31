---
id: TASK-327
title: Honor scan schedule policy intervals and flags in cve_worker
status: To Do
assignee: []
created_date: '2026-05-31 03:27'
updated_date: '2026-06-10 03:23'
labels:
  - backend
  - cve
  - scanning
  - worker
milestone: 'm-19: Design Parity Existing Surfaces'
dependencies:
  - TASK-326
references:
  - packages/default/src/builder/cve_worker.rs
  - packages/default/src/queries/cve_scans.rs
modified_files:
  - packages/default/src/builder/cve_worker.rs
  - packages/default/src/queries/cve_scans.rs
  - packages/default/src/queries/scanning_tests.rs
priority: high
ordinal: 288000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem
TASK-326 persists and exposes a scan schedule policy (`on_build`, `deployed_interval`, `recent_interval`, `archived_interval`, `archived_enabled`, `rebuild_to_scan`) and surfaces it in the Scanning view, but the CVE scanning worker does not yet honor these settings. The worker currently scans build-complete derivations on a fixed poll cadence regardless of the configured policy.

## Goal
Make the CVE scanning worker and target-selection query obey the persisted scan schedule policy so the Scanning UI controls are operational rather than display-only.

## Non-Goals
- No redesign of the Scanning UI itself beyond reflecting truthful worker behavior.
- No speculative new policy fields beyond those already persisted by TASK-326.
- No unrelated CVE workflow changes outside target selection, scheduling, and rebuild gating.

## Scope
1. Gate scan-on-build behavior behind `on_build`.
2. Use `deployed_interval`, `recent_interval`, and `archived_interval` to determine when completed scans become stale for each class.
3. Respect `archived_enabled` when selecting archived configs for automatic rescan.
4. Respect `rebuild_to_scan` when deciding whether uncached derivations should trigger rebuild-then-scan behavior.
5. Ensure queue/stats semantics remain consistent with the policy-driven worker behavior.

## Architectural Constraints
- Keep scheduling policy interpretation in backend worker/query logic, not UI code.
- Preserve auditable selection behavior so operators can explain why a target was or was not scanned.
- Reuse persisted singleton policy state from TASK-326 rather than introducing parallel config.

## Impact Areas
- `packages/default/src/builder/cve_worker.rs`
- `packages/default/src/queries/cve_scans.rs`
- scanning-related models/tests as needed

## Verification Plan
Tier 0/1 targeted verification:
- `nix develop -c env SQLX_OFFLINE=true cargo test --manifest-path packages/default/Cargo.toml cve_worker`
- `nix develop -c env SQLX_OFFLINE=true cargo test --manifest-path packages/default/Cargo.toml cve_scans`
- targeted integration verification of policy combinations against local dev DB
- Scanning UI/manual verification that configured policy changes produce the expected queue/staleness behavior

## Risk Level
High: policy mismatches can silently waste scanner capacity or leave fleets stale.

## Dependencies
- TASK-326 is merged and provides persisted schedule policy data.
- Freshness semantics remain: recency of vulnix scan, recent cutoff 30 days, stale determined by configured interval for the target class.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 When `on_build` is false, freshly built configs are not auto-enqueued solely by build completion
- [ ] #2 Automatic rescan selection uses the configured deployed/recent/archived intervals for stale detection
- [ ] #3 Archived configs are excluded from automatic rescan when `archived_enabled` is false
- [ ] #4 Needs-build targets are skipped instead of rebuilt when `rebuild_to_scan` is false
- [ ] #5 Targeted tests cover the policy combinations and selection outcomes
- [ ] #6 Scanning stats/queue behavior remains consistent with the new worker policy enforcement
<!-- AC:END -->

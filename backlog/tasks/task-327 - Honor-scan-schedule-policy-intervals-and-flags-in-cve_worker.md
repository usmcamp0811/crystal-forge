---
id: TASK-327
title: Honor scan schedule policy intervals and flags in cve_worker
status: Backlog
assignee: []
created_date: '2026-05-31 03:27'
labels:
  - backend
  - cve
  - scanning
  - worker
milestone: UI/UX Refresh
dependencies:
  - TASK-326
references:
  - packages/default/src/builder/cve_worker.rs
  - packages/default/src/queries/cve_scans.rs
priority: high
ordinal: 288000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

TASK-326 persists and exposes a scan schedule policy (on_build, deployed/recent/archived intervals, archived_enabled, rebuild_to_scan) and surfaces it in the Scanning view, but the CVE scanning worker (`packages/default/src/builder/cve_worker.rs` + `queries/cve_scans.rs::get_targets_needing_cve_scan`) does NOT yet honor these settings. The worker currently scans build-complete derivations on a fixed poll cadence regardless of configured freshness intervals or flags.

## Desired Outcome

The CVE scan worker selects and schedules rescans according to the persisted scan schedule policy:
- `on_build`: scan freshly-built configs before deploy (existing behavior gated by this flag).
- `deployed_interval` / `recent_interval` / `archived_interval`: rescan configs whose latest completed scan is older than the interval for their freshness/rotation class.
- `archived_enabled`: when false, never auto-rescan archived configs.
- `rebuild_to_scan`: when false, skip configs whose derivation is not in any cache (needs-build) instead of triggering a rebuild; when true, allow rebuild-then-scan.

## Context

- Depends on TASK-326 landing the `scan_schedule_policy` persistence + read path.
- High priority: should be picked up as soon as the Scanning view (TASK-326) merges, so the exposed policy controls become functional rather than display-only.
- Freshness classification semantics agreed for TASK-326: freshness = recency of the vulnix scan; "recent" cutoff = 30 days; "stale" = last successful scan older than configured interval for the class; "needs-build" = derivation not in any cache.
<!-- SECTION:DESCRIPTION:END -->

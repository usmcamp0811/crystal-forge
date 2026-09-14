---
id: TASK-326.1
title: Persist scan wait states and bounded vulnix execution logs
status: Backlog
assignee: []
created_date: '2026-09-09 03:31'
labels:
  - scanning
  - backend
  - api
  - database
  - observability
  - security
dependencies:
  - TASK-337
references:
  - git commit e1b7434899e23f43770632e59d80a76a8fc8459e
  - TASK-325
documentation:
  - docs/design/CrystalForge/components/ScanningView.jsx
  - docs/design/CrystalForge/data-scanning.js
modified_files:
  - packages/default/crates/cf-server/src/builder/cve_worker.rs
  - packages/default/crates/cf-server/src/queries/cve_scans.rs
  - packages/default/crates/cf-server/src/queries/scanning.rs
  - packages/default/crates/cf-server/src/handlers/api/scanning.rs
  - packages/default/crates/cf-server/src/api/
  - packages/default/migrations
  - packages/web-ui/src/api/models.rs
parent_task_id: TASK-326
priority: high
type: feature
ordinal: 472000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Design commit `e1b74348` distinguishes scans that are waiting for a realizable closure from terminal failures and adds an exact scan log surface. Extend the authoritative scan lifecycle so a requested scan can wait while a build is running or while its closure is unavailable from configured substituters, then start automatically when the closure becomes available. Persist bounded, redacted vulnix execution output and scan metadata so authorized clients can inspect real running and terminal logs without synthetic progress or fixture-generated lines. Preserve the execution ownership, deduplication, authorization, and recovery guarantees established by TASK-325.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The scan lifecycle represents waiting for a build and waiting for a reachable closure separately from queued running failed and completed states
- [ ] #2 A waiting scan remains deduplicated and starts automatically when its exact closure becomes available without requiring repeated user submissions
- [ ] #3 Retry cadence timeout cancellation stale recovery and terminal transitions preserve TASK-325 execution ownership and concurrency limits
- [ ] #4 Running scans expose their real start time and elapsed state without inventing percentage progress that vulnix does not provide
- [ ] #5 Bounded vulnix stdout and stderr plus exit status and sanitized failure context are persisted for the exact scan execution and remain associated with its configuration flake and full revision identity
- [ ] #6 Scan log persistence redacts secrets credentials signed URLs and sensitive store or repository metadata before database storage logging and API serialization
- [ ] #7 Authorized APIs return exact scan details and paged or bounded log content while hidden or unauthorized targets follow existing non-disclosing behavior
- [ ] #8 Manual check retry and rebuild-triggered scan actions are idempotent and report whether work was created reused or remains blocked
- [ ] #9 Additive migrations and SQLx metadata preserve supported deployments and existing scan records map to explicit compatible states
- [ ] #10 Focused lifecycle ownership retry redaction bounds authorization migration and API tests pass through the repository Nix environment and affected contracts are documented
<!-- AC:END -->

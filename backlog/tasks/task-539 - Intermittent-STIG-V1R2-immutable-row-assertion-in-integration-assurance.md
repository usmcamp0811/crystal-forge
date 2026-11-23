---
id: TASK-539
title: Intermittent STIG V1R2 immutable-row assertion in integration-assurance
status: Backlog
assignee: []
created_date: '2026-11-21'
labels: [testing, compliance, integration-assurance]
dependencies: []
references:
  - TASK-490.19
  - TASK-534.6
priority: high
---

## Description
<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

The `integration-assurance` case
`stig-v1r2-browser-import-preserves-all-v1r1-version-records`
(`checks/integration-assurance/scenarios/fragments/stig-import.py`) fails
intermittently with `V1R2 rewrote or removed an existing <table> row`.

Observed:

- MR !350 head `3b330b87`: GitLab job 18238700214 failed for
  `policy_requirement_mappings`; a farm run of the same head also failed.
- MR !348 head `6258a329` (a one-line `AGENTS.md` change on dev) failed the
  same case for `compliance_requirement_versions`, rows that migration 0220
  makes immutable once their digest is set.
- MR !351 and !352 (children of !350) passed the case.
- A farm run of `3b330b87` with diagnostics (a second snapshot taken before
  V1R2, plus row dumps on failure) passed; the two pre-V1R2 snapshots were
  identical (1052 rows).

Static review found no write path in the !350 diff and no code that updates
or deletes an existing `policy_requirement_mappings` row during a V1R2 import
(insert uses `ON CONFLICT DO NOTHING`; update and delete endpoints only touch
`manual` rows). The cause is not established. Candidates: a background writer
that changes V1R1 rows after the V1R1 case ends (for example a digest backfill
of `pending` columns), or a read-side race in the snapshot.

## Desired outcome

The assertion reports the exact differing row (before row, after row with the
same id, per-table counts) and the root cause is fixed or the comparison is
narrowed to provably immutable columns with an approved reason. Historical
V1R1 evidence must stay byte-for-byte unchanged by a V1R2 import.
<!-- SECTION:DESCRIPTION:END -->

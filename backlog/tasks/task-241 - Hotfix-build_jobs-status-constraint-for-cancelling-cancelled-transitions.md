---
id: TASK-241
title: Hotfix build_jobs status constraint for cancelling/cancelled transitions
status: Done
assignee: []
created_date: '2026-04-03 12:18'
updated_date: '2026-04-06 12:51'
labels:
  - hotfix
  - builds
  - database
  - migration
dependencies: []
references:
  - packages/default/migrations/0083_create_builders_infrastructure.sql
  - packages/default/src/queries/builders.rs
  - packages/default/src/handlers/api/builders.rs
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

The merged cancel lifecycle code in TASK-238 updates `build_jobs.status` to `cancelling` and `cancelled`, but the database schema still enforces the original CHECK constraint from migration `0083_create_builders_infrastructure.sql`:

```sql
status TEXT NOT NULL CHECK (status IN ('queued', 'building', 'success', 'failed'))
```

On deployed `dev`, clicking Stop on a running build now reaches the server-side cancel handler, but the `UPDATE build_jobs SET status = 'cancelling' ...` fails at the database layer because the new statuses are not permitted. The UI surfaces this as:

- `HTTP 400: Failed to cancel build job`

Server logs confirm the sequence:
1. `SELECT * FROM build_jobs WHERE id = $1` succeeds
2. `UPDATE build_jobs SET status = $2 ... RETURNING *` affects 0 rows / fails
3. request returns 400

## Goal

Ship an urgent hotfix that updates the database schema to allow `cancelling` and `cancelled` in `build_jobs.status`, and verify that cancel on a running build no longer fails at the DB layer.

## Non-Goals

- No new UI work
- No rework of builder-side cancel polling (already merged)
- No role/auth changes
- No log-append `cancelling` follow-up (that remains TASK-239)

## Scope

1. Add a new migration that drops and recreates the `build_jobs.status` CHECK constraint to allow:
   - `queued`
   - `building`
   - `cancelling`
   - `cancelled`
   - `success`
   - `failed`

2. Verify partial indexes still make sense:
   - queue index remains `WHERE status = 'queued'`
   - active builder index may remain `WHERE status = 'building'` unless `cancelling` needs inclusion for concurrency logic

3. Audit queries for assumptions about allowed statuses where necessary, but keep the hotfix minimal.

4. Run the required migration / sqlx sync workflow if SQLx metadata is impacted.

## Verification Plan

### Tier 0
- `nix develop -c env SQLX_OFFLINE=true cargo check --package crystal-forge`
- If sqlx sync applies: `nix develop`, `db-only up`, `cargo sqlx prepare`

### Tier 1
- Apply migration on dev DB
- Click Stop on a currently building job
- Verify DB status transitions to `cancelling` instead of returning 400
- Verify builder later transitions it to `cancelled`

## Impact Areas

- `packages/default/migrations/` — new migration file
- Possibly `sqlx-data.json` / SQLx metadata if required by repo workflow

## Risk Level

High (hotfix on deployed dev DB path), but code scope is small and isolated to schema validation.

## References

- `packages/default/migrations/0083_create_builders_infrastructure.sql`
- `packages/default/src/queries/builders.rs`
- `packages/default/src/handlers/api/builders.rs`
- TASK-238 / MR !206
<!-- SECTION:DESCRIPTION:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
LOCK: openai-gpt-5.4 on reckless in /home/mcamp/code/crystal-forge/TASK-241-build-jobs-status-hotfix

## MR Created

MR !207: https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/207

Verification completed:
- `nix develop ../.. -c env SQLX_OFFLINE=true cargo check` passed
- Dev DB migrations applied successfully
- `cargo sqlx prepare` was run against the initialized dev DB

This hotfix is schema-only and should unblock `Stop` on running builds by allowing `cancelling`/`cancelled` at the database layer.

Marked Done after merge into dev (merge commit 3d18935b).
<!-- SECTION:NOTES:END -->

---
id: TASK-451
title: Reconcile the duplicated and divergent SQLx offline query caches
status: Backlog
assignee: []
created_date: '2026-08-31 23:23'
labels: []
dependencies: []
documentation:
  - docs/agents/database-safety.md
  - packages/default/WORKSPACE.md
priority: medium
type: chore
ordinal: 462000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

The backend workspace contains two SQLx offline query caches:

- `packages/default/.sqlx` at the workspace root, with 144 query files
- `packages/default/crates/cf-server/.sqlx` at the crate root, with 139 query files

The crate-level cache is a strict subset of the workspace-root cache. It is missing exactly these five queries:

- `query-511d1f01cd44f2755a586af3bc20d1cee83615261e4207b1e83dcb7ad398d1e2.json`
- `query-5e900542d9e4554dca7156995dbb9c72b55d5a8be5d7eff740b12d5c37beecb9.json`
- `query-765cff0ae03a51aa2e4fce4a9667b879b9065e3bf4ec0b4849c4008b892ae25b.json`
- `query-85a5cf9607c81f2589b24f507adf0d5a2eca7ad33959e1fd8ac3ee7a64494f20.json`
- `query-aa348bdac8c2a1680331990c04d855857d5f435d6fd0c39603dadd003876f37d.json`

`packages/default/WORKSPACE.md:145` states that server query metadata lives at `crates/cf-server/.sqlx/`. That statement is not correct in practice. This was discovered during TASK-450.1: filtering the server source tree to exclude the workspace-root `.sqlx` produced five build failures reading `set DATABASE_URL to use query macros online, or run cargo sqlx prepare to update the query cache`, one for each missing query.

TASK-450.1 resolved the immediate breakage by including the workspace-root `.sqlx` in the server source closure, and documented that decision in `packages/default/default.nix`. The underlying duplication is untouched.

## Why this matters

Two caches that can disagree is a correctness hazard. A future developer who follows `WORKSPACE.md` and runs preparation against the crate directory can produce a tree that builds locally but fails in a Nix build, or the reverse. It also makes it impossible to tell which cache is authoritative when they diverge.

## Goal

Have exactly one authoritative SQLx offline query cache for the backend workspace, with documentation that matches the real resolution behavior.

## Non-goals

- Changing any SQL query, migration, or schema.
- Changing the source filtering introduced by TASK-450.1 beyond whatever this reconciliation makes correct.

## Constraints

- SQLx preparation requires a database. Follow `docs/agents/database-safety.md`. Perform preparation only against a verified isolated local development database started by this repository.
- Determine the real resolution order that SQLx uses for this workspace and crate layout before deleting either cache. Do not delete a cache on the assumption that the other is authoritative.
- If the workspace-root cache remains authoritative, the server source filter may keep including it, but `WORKSPACE.md` must be corrected.

## Verification plan

- Build the server package and confirm no query macro errors.
- Confirm the retained cache contains every query the workspace expands.
- Confirm `WORKSPACE.md` describes the actual location and the actual preparation command.

## Impact areas

`packages/default/.sqlx`, `packages/default/crates/cf-server/.sqlx`, `packages/default/WORKSPACE.md`, and the server source filter in `packages/default/default.nix`.

## Risk level

Low to medium. The failure mode is a build error rather than silent incorrect behavior, but preparation requires database access and must follow the database safety rules.

## Dependencies

None. TASK-450.1 already made the build correct; this task removes the underlying ambiguity.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The backend workspace has exactly one authoritative SQLx offline query cache, or the reason two are required is documented
- [ ] #2 The retained cache contains every query the workspace expands, proven by a clean server build with no query macro errors
- [ ] #3 WORKSPACE.md describes the actual cache location and the actual preparation command
- [ ] #4 The server source filter in packages/default/default.nix matches the reconciled layout and its comment is accurate
- [ ] #5 Any SQLx preparation performed used a verified isolated local development database
<!-- AC:END -->

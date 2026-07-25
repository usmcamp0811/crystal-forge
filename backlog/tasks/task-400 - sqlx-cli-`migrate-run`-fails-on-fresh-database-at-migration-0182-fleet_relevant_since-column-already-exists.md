---
id: TASK-400
title: >-
  sqlx-cli `migrate run` fails on fresh database at migration 0182
  (fleet_relevant_since column already exists)
status: Backlog
assignee: []
created_date: '2026-07-25 01:38'
labels: []
dependencies: []
priority: medium
type: bug
ordinal: 399000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem
Running `sqlx migrate run` (the `sqlx-cli` binary, via `DATABASE_URL=... sqlx migrate run` from `packages/default/crates/cf-server`) against a completely fresh, empty PostgreSQL database fails deterministically at migration `0182_cve_fleet_relevant_since.sql` with:

```
error: while executing migration 182: error returned from database: column "fleet_relevant_since" of relation "cves" already exists
```

This reproduces on a brand-new database with no prior state, and after the failure the `_sqlx_migrations` tracking table does not exist at all (nothing appears to have been durably committed), so this is not a matter of re-running an already-partially-applied database.

Only one migration file (`0182_cve_fleet_relevant_since.sql`) references `fleet_relevant_since`, so it is not an obvious duplicate-migration issue. It was independently observed by two separate agent sessions while validating TASK-399 (retry policy / migrations 0183-0185), both starting from fresh databases and using the pinned `sqlx-cli` from the Nix devshell.

Notably, `#[sqlx::test(migrations = "./migrations")]`-driven tests (which use SQLx's library-level migrator rather than the `sqlx-cli` binary) succeed running the full migration chain including 0182 and TASK-399's new 0183/0184/0185 migrations without this error. This suggests a `sqlx-cli`-specific issue (possibly a version/behavior mismatch, transaction handling difference, or a stale internal migration listing) rather than a problem with the SQL in 0182 itself.

## Desired outcome
`sqlx migrate run` from the repository's Nix devshell should apply the full migration chain cleanly against a fresh database, matching the behavior already exhibited by the library-level migrator. This affects local developer workflows that rely on the `sqlx-cli` binary directly (e.g. manual `sqlx migrate run`/`sqlx-refresh` outside of `#[sqlx::test]`), and blocked ad hoc validation of pre-existing `#[tokio::test]`-based DB tests that expect a `sqlx-cli`-migrated database.

## Suggested investigation
- Compare the `sqlx-cli` version pinned in the flake devshell against the `sqlx` library version used by `cf-server`.
- Check whether `0182_cve_fleet_relevant_since.sql` (or an adjacent migration) interacts badly with `sqlx-cli`'s transaction/checksum handling.
- Reproduce with `sqlx migrate run -v` for more detail, and compare against the library migrator's behavior in `#[sqlx::test]`.

## Non-goals
Not blocking for TASK-399 delivery; TASK-399's own migrations (0183-0185) were independently validated via the library-level migrator (`#[sqlx::test]`) in an isolated PostgreSQL cluster and applied successfully.
<!-- SECTION:DESCRIPTION:END -->

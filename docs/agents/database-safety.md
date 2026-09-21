# Database and SQLx Safety

SQLx offline metadata must remain synchronized with the schema and compile-time checked queries.

## Isolated local development database: two current mechanisms

This repository currently has two ways to get an isolated local
development PostgreSQL instance:

- The process-compose-based `db-only`/`run-ui-dev` workflow this document
  otherwise describes: a fixed port (`3042`) shared by every worktree,
  with `packages/devScripts/db-usability-check.sh` verifying at startup
  that the process actually answering that port belongs to the current
  worktree before reuse.
- The devenv-based workflow added in TASK-462.1
  (`docs/agents/devenv-workflow.md`): a dynamically allocated,
  genuinely per-worktree PostgreSQL instance and port, with its own
  per-worktree on-disk data directory. Two worktrees running this
  workflow at the same time cannot collide on a port or data directory in
  the first place, so no equivalent ownership check is needed there.

The rest of this document describes the process-compose-based workflow.
It remains fully supported; TASK-462.1 does not remove or weaken it. See
`docs/agents/devenv-workflow.md` for the devenv-based alternative,
including how to find its resolved database port for a given worktree.

## When preparation is required

Run SQLx preparation when a change affects any of:

- Database migrations or schema
- SQL checked at compile time
- Selected columns or aliases
- Bind parameters or their types
- Query result shapes
- Models consumed directly by checked queries

If inspection shows that a change cannot affect SQLx metadata, record that conclusion rather than refreshing metadata unnecessarily.

## Required environment

Enter the repository environment and start its isolated development database using the current process-compose helpers. Typical flow:

```bash
nix develop
db-only up
cargo sqlx prepare
```

Use repository helpers such as `sqlx-prepare` when they encode additional required configuration. Confirm their current behavior before relying on them.

Do not prepare metadata against:

- A shared developer database
- Integration, staging, or production
- An arbitrary PostgreSQL instance on its default port
- A database whose ownership and isolation cannot be verified

## Destructive refreshes

Helpers such as `sqlx-refresh` or `sqlx database reset` may drop and recreate data. Before running one, verify and state:

- The resolved `DATABASE_URL` host and port
- The database name
- That the service was started by this repository's local process-compose configuration
- That no shared or persistent user data is present

If any point cannot be verified, do not run the destructive helper. Try non-destructive preparation and report the blocker if it is insufficient.

Never print credentials embedded in `DATABASE_URL`. Redact secrets while retaining enough host, port, and database information to establish isolation.

## Schema changes

- Add a new migration using repository naming and ordering conventions.
- Do not rewrite an applied migration unless the repository explicitly permits it and the active task requires it.
- Exercise both migration application and the affected runtime behavior.
- Consider upgrade compatibility for existing installations, not only a fresh database.
- Update checked-query metadata after the migration is applied locally.

## Completion gate

Before review, confirm:

- Required migrations apply successfully.
- SQLx preparation succeeds.
- Generated metadata changes are included and limited to intended queries.
- Relevant tests/builds pass in offline mode where production builds depend on it.

If metadata cannot be generated or verified, do not move the task to `Review`.

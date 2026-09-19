# run-ui-dev Database Behavior Check

This check is a regression test for two scripts under
`packages/devScripts/` that `run-ui-dev` (the local UI development harness)
uses to decide how to start its development database. No VM, and no real
PostgreSQL instance, is started; `psql` is mocked.

## What it verifies

- `db-usability-check.sh` — the probe `run-ui-dev` runs before trusting an
  already-running PostgreSQL instance as its dev database. `pg_isready`
  alone only proves a PostgreSQL process answers on a port; it does not
  prove the `crystal_forge` role can use the `crystal_forge` database there,
  or that the process belongs to the current worktree at all. This test
  proves the probe script tells those cases apart, using a real
  `ss`/`/proc`-based worktree-identity lookup against real background
  processes the test itself spawns (only `psql` is mocked).
- `db-only-start-test.sh` — proves `db-only-start.sh` starts the db-only
  service with `$PROJECT_ROOT` as its working directory, not the (possibly
  nested) directory `run-ui-dev` was invoked from. Without this, the
  relative `dataDir` (`./data/db`) would resolve to a different location
  than the worktree-identity check expects, silently diverging.

## Why it is a separate check

These two scripts gate whether `run-ui-dev` reuses or replaces a running
database, a decision with real risk of connecting a developer's UI session
to the wrong worktree's data. Testing that gate directly, without going
through the full `run-ui-dev` startup sequence or a real PostgreSQL
instance, keeps this fast and focused on the identity/working-directory
logic itself.

## Run it

```sh
nix build .#checks.x86_64-linux.run-ui-dev-db-check --print-build-logs
```

No VM, no real PostgreSQL. The check runs two bash test scripts against the
real production scripts they test, with `bash`, `coreutils`, and `iproute2`
available in the sandbox for the real `ss`/`/proc` worktree-identity checks.

## Out of scope

- This does not start or exercise a real PostgreSQL server, `run-ui-dev`
  itself, or the web UI development stack it manages.

## CI

Part of the `.gitlab-ci.yml` `flake-check` matrix
(`CHECK_NAME: run-ui-dev-db-check`), so it runs on every merge request and on
`main`.

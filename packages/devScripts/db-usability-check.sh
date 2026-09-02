#!/usr/bin/env bash

# Verifies that an already-reachable PostgreSQL instance is actually safe for
# `run-ui-dev` to reuse, not merely that a PostgreSQL process answers
# `pg_isready` on its port.
#
# `run-ui-dev` calls this script only after `pg_isready` already succeeded
# against the target host/port. `pg_isready` proves a PostgreSQL process
# answers the wire protocol there; it proves nothing about which database
# that process actually serves. Two independent questions must both be
# answered before reuse is safe, and this script answers them as two
# distinct, ordered checks rather than one combined heuristic:
#
#   1. Identity: is the connected PostgreSQL instance the one this worktree
#      itself would start? Each worktree gets its own on-disk PostgreSQL
#      data directory (this repository's db-only bootstrap defaults to
#      `./data/db` under the invoking project root — see
#      `dbOnly.config.services.postgres.db.dataDir` in default.nix), but the
#      dev database port is a single fixed constant shared by every
#      worktree. A different worktree's `db-only` PostgreSQL process left
#      running can answer on that same port, present valid crystal_forge
#      credentials, and pass every schema/privilege check below, while still
#      being the wrong database entirely. Attaching to it would let this
#      worktree read, migrate, or seed another worktree's data.
#   2. Usability: can the crystal_forge role actually use *this* database?
#      This is the check that catches the originally reported failure: a
#      database whose public-schema objects are owned by an unrelated role
#      (for example the invoking OS user instead of crystal_forge), which
#      makes cf-server fail migrations with "permission denied for table
#      _sqlx_migrations" well after startup looked successful.
#
# Both checks must pass before the caller may reuse the connected instance.
# Passing identity does not imply usability, and passing usability does not
# imply identity; a database can fail either independently.
#
# Exit status:
#   0  both checks passed; the caller may proceed.
#   1  the instance is not this worktree's own database, is not usable, or
#      could not be queried at all (for example, wrong role, wrong
#      password, or a permission error). The caller must not start the
#      server against this database.
#
# This script never creates, drops, resets, or otherwise mutates any
# database, including one that fails these checks. It only reads identity
# and privilege metadata, so it is safe to run every time `run-ui-dev`
# starts, including against a database a developer is actively using
# (its own or a different worktree's).

set -euo pipefail

host="${1:?usage: db-usability-check.sh HOST PORT USER PASSWORD DBNAME EXPECTED_DATA_DIR}"
port="${2:?usage: db-usability-check.sh HOST PORT USER PASSWORD DBNAME EXPECTED_DATA_DIR}"
user="${3:?usage: db-usability-check.sh HOST PORT USER PASSWORD DBNAME EXPECTED_DATA_DIR}"
password="${4:?usage: db-usability-check.sh HOST PORT USER PASSWORD DBNAME EXPECTED_DATA_DIR}"
dbname="${5:?usage: db-usability-check.sh HOST PORT USER PASSWORD DBNAME EXPECTED_DATA_DIR}"
expected_data_dir="${6:?usage: db-usability-check.sh HOST PORT USER PASSWORD DBNAME EXPECTED_DATA_DIR}"

# ── 1. Identity: does this instance belong to this worktree? ───────────────
#
# `SHOW data_directory` reports the on-disk directory the *connected*
# PostgreSQL instance was started against, which is the same identity
# services-flake resolves `dataDir` into at postgres startup
# (`readlink -f "${dataDir}"`; see the services-flake postgres service
# module). Reading it requires the `pg_read_all_settings` role, which this
# repository's db-only bootstrap grants to `crystal_forge` specifically so
# this check can run with the same unprivileged credentials the server
# uses — see the `db-module`/`db-core-module` `initialScript.before` in
# default.nix. A database that predates that grant (or a genuinely foreign,
# non-Crystal-Forge PostgreSQL instance) cannot be proven to be this
# worktree's own database, so a query failure here is treated as a
# worktree-identity failure, not skipped.
if ! actual_data_dir="$(PGPASSWORD="$password" psql -h "$host" -p "$port" -U "$user" -d "$dbname" \
  -v ON_ERROR_STOP=1 -A -t -c 'SHOW data_directory;' 2>&1)"; then
  printf 'db-usability-check: could not determine which PostgreSQL data directory role %s is using at %s:%s/%s:\n\n%s\n\nThis can also mean role %s is missing the pg_read_all_settings grant this repository db-only bootstrap creates, most commonly because the database predates that grant.\n' \
    "$user" "$host" "$port" "$dbname" "$actual_data_dir" "$user" >&2
  exit 1
fi

# Normalize both sides the same way before comparing: `readlink -m`
# resolves symlinks and relative components without requiring the target to
# exist, so an expected directory this worktree has never actually created
# yet (a totally fresh worktree finding a *different* worktree's instance
# already listening) still compares correctly against the real, existing
# directory PostgreSQL reports.
normalized_actual="$(readlink -m -- "$actual_data_dir")"
normalized_expected="$(readlink -m -- "$expected_data_dir")"

if [[ "$normalized_actual" != "$normalized_expected" ]]; then
  printf 'db-usability-check: PostgreSQL at %s:%s is a different Crystal Forge database than this worktree expects.\n\n  Expected data directory (this worktree): %s\n  Actual data directory (connected instance): %s\n\nThis usually means another worktree db-only PostgreSQL instance is listening on this port. It is left untouched: not migrated, not seeded, not reset, not stopped.\n\nRecovery:\n  1. From the worktree that owns the actual data directory above, stop its instance: db-only down\n  2. Re-run run-ui-dev from this worktree; it will bootstrap its own database.\n' \
    "$host" "$port" "$normalized_expected" "$normalized_actual" >&2
  exit 1
fi

# ── 2. Usability: can crystal_forge actually use this database? ────────────
#
# Can the role create objects in schema `public` (required for a fresh
# migration run) and, if migrations already ran, read its own
# `_sqlx_migrations` table? An empty, freshly bootstrapped database has no
# `_sqlx_migrations` table yet — that is a normal, usable starting state,
# so the migration-table check only applies once that table exists.
probe_sql="SELECT has_schema_privilege('$user', 'public', 'CREATE') AND COALESCE((SELECT has_table_privilege('$user', 'public._sqlx_migrations', 'SELECT') FROM pg_tables WHERE schemaname = 'public' AND tablename = '_sqlx_migrations'), TRUE);"

if ! result="$(PGPASSWORD="$password" psql -h "$host" -p "$port" -U "$user" -d "$dbname" \
  -v ON_ERROR_STOP=1 -A -t -c "$probe_sql" 2>&1)"; then
  printf 'db-usability-check: could not query %s as role %s at %s:%s:\n\n%s\n' \
    "$dbname" "$user" "$host" "$port" "$result" >&2
  exit 1
fi

if [[ "$result" != "t" ]]; then
  printf 'db-usability-check: role %s cannot create in schema public (or cannot read its own _sqlx_migrations table) on database %s at %s:%s.\n' \
    "$user" "$dbname" "$host" "$port" >&2
  exit 1
fi

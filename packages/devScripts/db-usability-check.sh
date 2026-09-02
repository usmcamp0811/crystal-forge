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
#      worktree read, migrate, or seed another worktree's data. This check
#      is answered entirely at the OS level (which process owns the port),
#      not by querying the database, so it applies identically to a
#      database created before this check existed and one created after.
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
#      could not be inspected at all (for example, the listening process
#      exited, or the database rejected the credentials). The caller must
#      not start the server against this database.
#
# This script never creates, drops, resets, or otherwise mutates any
# database or process, including one that fails these checks. It only
# reads process and privilege metadata, so it is safe to run every time
# `run-ui-dev` starts, including against a database a developer is
# actively using (its own or a different worktree's).

set -euo pipefail

host="${1:?usage: db-usability-check.sh HOST PORT USER PASSWORD DBNAME EXPECTED_DATA_DIR}"
port="${2:?usage: db-usability-check.sh HOST PORT USER PASSWORD DBNAME EXPECTED_DATA_DIR}"
user="${3:?usage: db-usability-check.sh HOST PORT USER PASSWORD DBNAME EXPECTED_DATA_DIR}"
password="${4:?usage: db-usability-check.sh HOST PORT USER PASSWORD DBNAME EXPECTED_DATA_DIR}"
dbname="${5:?usage: db-usability-check.sh HOST PORT USER PASSWORD DBNAME EXPECTED_DATA_DIR}"
expected_data_dir="${6:?usage: db-usability-check.sh HOST PORT USER PASSWORD DBNAME EXPECTED_DATA_DIR}"

# ── 1. Identity: does this instance belong to this worktree? ───────────────
#
# `SHOW data_directory` would report the on-disk directory the connected
# PostgreSQL instance was started against, but reading it requires the
# pg_read_all_settings role, which a legitimate database created before
# this check existed would never have. Asking the *database* to prove its
# own identity is also unnecessarily indirect: what actually matters is
# which PostgreSQL process is bound to this port, a fact the operating
# system already knows without any database privilege at all. `ss` finds
# the PID of the process listening on the target port; PostgreSQL chdir()s
# into its data directory at startup and keeps it as its cwd for the life
# of the process (this is also how services-flake resolves a relative
# `dataDir` — see `dbOnly.config.services.postgres.db.dataDir` in
# default.nix), so `/proc/<pid>/cwd` is exactly that directory. This works
# identically for a brand-new database and one created long before this
# check existed: no grant, no marker table, no schema change of any kind
# is required on the database itself.
listening_pid="$(ss -H -tlnp "sport = :$port" 2>/dev/null | grep -oE 'pid=[0-9]+' | head -n1 | cut -d= -f2)" || true
if [[ -z "$listening_pid" ]]; then
  printf 'db-usability-check: could not find the process listening on %s:%s (ss reported none). It may have exited between pg_isready and this check; try again.\n' \
    "$host" "$port" >&2
  exit 1
fi

if ! actual_data_dir="$(readlink -f -- "/proc/$listening_pid/cwd" 2>&1)"; then
  printf 'db-usability-check: could not read the working directory of the process listening on %s:%s (pid %s, /proc/%s/cwd): %s\n\nThis can mean that process exited, or belongs to a different OS user than this worktree is running as.\n' \
    "$host" "$port" "$listening_pid" "$listening_pid" "$actual_data_dir" >&2
  exit 1
fi

# Normalize the expected side the same way before comparing: `readlink -m`
# resolves symlinks and relative components without requiring the target to
# exist, so an expected directory this worktree has never actually created
# yet (a totally fresh worktree finding a *different* worktree's instance
# already listening) still compares correctly against the real, existing
# directory the listening process reports. `readlink -f` already fully
# canonicalized the actual side above (and requires it to exist).
normalized_actual="$actual_data_dir"
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

#!/usr/bin/env bash

# Verifies that the Crystal Forge development database is actually usable,
# not merely that a PostgreSQL process answers `pg_isready` on its port.
#
# `run-ui-dev` calls this script only after `pg_isready` already succeeded
# against the target host/port. `pg_isready` proves a PostgreSQL process
# answers the wire protocol there; it does not prove that process is the
# `crystal_forge`-owned database this repository's `db-only` bootstrap
# creates. A stray or foreign PostgreSQL instance can be listening on the
# same well-known port instead — for example a different worktree's
# `db-only` process left running, since the dev database port is a single
# fixed constant shared by every worktree while each worktree's data
# directory is separate. When that happens the `crystal_forge` role can be
# unable to read or write its own migration history table, and the server
# fails much later with a hard-to-diagnose "permission denied for table
# _sqlx_migrations" error instead of a clear, early one.
#
# This script probes usability directly with the exact credentials the
# server will use: can the role create objects in schema `public` (required
# for a fresh migration run), and, if migrations already ran, can it read
# its own `_sqlx_migrations` table? An empty, freshly bootstrapped database
# has no `_sqlx_migrations` table yet — that is a normal, usable starting
# state, so the migration-table check only applies once that table exists.
#
# Exit status:
#   0  the database is usable; the caller may proceed.
#   1  the database is not usable, or could not be queried at all (for
#      example, wrong role, wrong password, or a permission error). The
#      caller must not start the server against this database.
#
# This script never creates, drops, or otherwise mutates any database. It
# only reads privilege metadata, so it is safe to run every time `run-ui-dev`
# starts, including against a database a developer is actively using.

set -euo pipefail

host="${1:?usage: db-usability-check.sh HOST PORT USER PASSWORD DBNAME}"
port="${2:?usage: db-usability-check.sh HOST PORT USER PASSWORD DBNAME}"
user="${3:?usage: db-usability-check.sh HOST PORT USER PASSWORD DBNAME}"
password="${4:?usage: db-usability-check.sh HOST PORT USER PASSWORD DBNAME}"
dbname="${5:?usage: db-usability-check.sh HOST PORT USER PASSWORD DBNAME}"

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

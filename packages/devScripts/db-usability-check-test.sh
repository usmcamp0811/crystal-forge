#!/usr/bin/env bash

# Regression coverage for db-usability-check.sh.
#
# `pg_isready` proves a PostgreSQL process answers on a host/port; it does
# not prove the `crystal_forge` role can use the `crystal_forge` database
# there. These cases prove the script tells those two things apart: a
# reachable-but-incompatible database must fail with a clear diagnostic
# instead of being reported as usable.

set -euo pipefail

script="${1:?db-usability-check.sh path is required}"
tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT

mock_bin="$tmp_dir/bin"
mkdir -p "$mock_bin"

# Records the exact connection arguments it was called with so tests can
# assert the script passed through the caller's host/port/user/db instead of
# hard-coding a second configuration.
printf '#!%s\n' "$(command -v bash)" >"$mock_bin/psql"
cat >>"$mock_bin/psql" <<'SH'

printf '%s\n' "$*" >>"$PSQL_CALL_LOG"
if [[ "${MOCK_PSQL_STATUS:-0}" -ne 0 ]]; then
  echo "${MOCK_PSQL_ERROR:-mock psql connection failure}" >&2
  exit "${MOCK_PSQL_STATUS}"
fi
printf '%s' "${MOCK_PSQL_RESULT:-t}"
SH
chmod +x "$mock_bin/psql"

export PATH="$mock_bin:$PATH"

run_success() {
  local case_name="$1"
  shift
  PSQL_CALL_LOG="$tmp_dir/$case_name.psql-calls" \
    bash "$script" "$@" >"$tmp_dir/$case_name.stdout" 2>"$tmp_dir/$case_name.stderr"
}

run_failure() {
  local case_name="$1"
  shift
  if PSQL_CALL_LOG="$tmp_dir/$case_name.psql-calls" \
    bash "$script" "$@" >"$tmp_dir/$case_name.stdout" 2>"$tmp_dir/$case_name.stderr"; then
    printf 'Expected %s to fail.\n' "$case_name" >&2
    exit 1
  fi
}

assert_contains() {
  local file="$1"
  local expected="$2"
  if [[ "$(<"$file")" != *"$expected"* ]]; then
    printf 'Expected %s to contain: %s\n' "$file" "$expected" >&2
    exit 1
  fi
}

# ── PostgreSQL responds, expected DB is valid → proceed ─────────────────────
# Covers both a freshly bootstrapped (no migrations yet) and an existing,
# correctly initialized dev database: both cases are represented by the same
# probe query result (`t`) from this script's point of view.

run_success usable-db 127.0.0.1 3042 crystal_forge password crystal_forge
assert_contains "$tmp_dir/usable-db.psql-calls" "-h 127.0.0.1 -p 3042 -U crystal_forge -d crystal_forge"

# ── PostgreSQL responds, but the role cannot use the migration table/schema
#    → fail with a clear diagnostic ─────────────────────────────────────────
# This is the reported failure mode: a foreign or ownership-incompatible
# database answers pg_isready, but role crystal_forge lacks the privileges
# our probe query checks for.

MOCK_PSQL_RESULT=f run_failure unusable-db 127.0.0.1 3042 crystal_forge password crystal_forge
assert_contains "$tmp_dir/unusable-db.stderr" 'cannot create in schema public'

# ── The role/database cannot even be queried (for example, wrong password,
#    unknown role, or a permission error on the probe query itself) → fail
#    with the underlying database error surfaced, not swallowed ────────────

MOCK_PSQL_STATUS=2 MOCK_PSQL_ERROR='FATAL: password authentication failed for user "crystal_forge"' \
  run_failure unreachable-db 127.0.0.1 3042 crystal_forge password crystal_forge
assert_contains "$tmp_dir/unreachable-db.stderr" 'could not query crystal_forge as role crystal_forge'
assert_contains "$tmp_dir/unreachable-db.stderr" 'password authentication failed'

# ── Connection details pass through unchanged, including a non-default host
#    and port (a different worktree's isolated instance, for example) ──────

run_success custom-connection 10.1.2.3 6543 crystal_forge secret crystal_forge
assert_contains "$tmp_dir/custom-connection.psql-calls" "-h 10.1.2.3 -p 6543 -U crystal_forge -d crystal_forge"

printf 'db-usability-check tests passed.\n'

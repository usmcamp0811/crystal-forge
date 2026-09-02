#!/usr/bin/env bash

# Regression coverage for db-usability-check.sh.
#
# `pg_isready` proves a PostgreSQL process answers on a host/port; it proves
# nothing about which database that process actually serves. The script
# under test answers two independent questions before `run-ui-dev` may
# reuse a connected instance: is it this worktree's own database (identity),
# and can the crystal_forge role actually use it (usability)? These cases
# prove each is enforced on its own, not collapsed into one heuristic, and
# that a reachable-but-wrong-or-incompatible database fails with a clear,
# specific diagnostic instead of being reported as reusable.
#
# Case A ("no PostgreSQL running") is intentionally not covered here: this
# script's contract assumes the caller already confirmed PostgreSQL is
# reachable (see its header comment and run-ui-dev's call site), so a
# reachability failure exercises run-ui-dev's unchanged pg_isready/db-only
# bootstrap loop, not this script. That path was verified manually against a
# real PostgreSQL instance (see the task record).

set -euo pipefail

script="${1:?db-usability-check.sh path is required}"
tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT

mock_bin="$tmp_dir/bin"
mkdir -p "$mock_bin"

# Two distinct queries now run against the same connection: `SHOW
# data_directory` (identity) and the has_schema_privilege/has_table_privilege
# probe (usability). The mock tells them apart by SQL text so a test can
# control each independently, mirroring how a real PostgreSQL instance
# answers each query on its own merits. It also records every call it
# receives, in order, so a test can prove the usability probe genuinely ran
# (or genuinely did not run) rather than only inspecting the final result.
printf '#!%s\n' "$(command -v bash)" >"$mock_bin/psql"
cat >>"$mock_bin/psql" <<'SH'

printf '%s\n' "$*" >>"$PSQL_CALL_LOG"
if [[ "${MOCK_PSQL_STATUS:-0}" -ne 0 ]]; then
  echo "${MOCK_PSQL_ERROR:-mock psql connection failure}" >&2
  exit "${MOCK_PSQL_STATUS}"
fi
case "$*" in
  *data_directory*)
    if [[ "${MOCK_DATA_DIRECTORY_STATUS:-0}" -ne 0 ]]; then
      echo "${MOCK_DATA_DIRECTORY_ERROR:-mock permission denied for data_directory}" >&2
      exit "${MOCK_DATA_DIRECTORY_STATUS}"
    fi
    printf '%s' "${MOCK_DATA_DIRECTORY:-/mock/worktree-a/data/db}"
    ;;
  *)
    printf '%s' "${MOCK_PSQL_RESULT:-t}"
    ;;
esac
SH
chmod +x "$mock_bin/psql"

export PATH="$mock_bin:$PATH"

# Every case below targets the same expected data directory unless a case
# explicitly overrides MOCK_DATA_DIRECTORY to simulate a different worktree.
default_expected_dir="/mock/worktree-a/data/db"

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

assert_line_count() {
  local file="$1"
  local expected="$2"
  local actual
  actual="$(wc -l <"$file")"
  if [[ "$actual" -ne "$expected" ]]; then
    printf 'Expected %s to have %s lines, got %s.\n' "$file" "$expected" "$actual" >&2
    exit 1
  fi
}

# ── Case B: correct worktree identity + usable database → reuse ────────────
# Covers both a freshly bootstrapped (no migrations yet) and an existing,
# correctly initialized dev database: both are represented by the same probe
# query result (`t`) from this script's point of view.

run_success usable-db 127.0.0.1 3042 crystal_forge password crystal_forge "$default_expected_dir"
assert_contains "$tmp_dir/usable-db.psql-calls" "-h 127.0.0.1 -p 3042 -U crystal_forge -d crystal_forge"
assert_line_count "$tmp_dir/usable-db.psql-calls" 2

# ── Case C: correct worktree identity, but application privileges/ownership
#    are incompatible → reject ──────────────────────────────────────────────
# Reproduces the originally reported failure mode: a database whose
# public-schema objects are owned by an unrelated role answers pg_isready
# and reports the expected worktree identity, but role crystal_forge lacks
# the privileges the usability probe checks for.

MOCK_PSQL_RESULT=f run_failure incompatible-ownership \
  127.0.0.1 3042 crystal_forge password crystal_forge "$default_expected_dir"
assert_contains "$tmp_dir/incompatible-ownership.stderr" 'cannot create in schema public'

# ── Case D: valid Crystal Forge database, but it belongs to another
#    worktree → reject ──────────────────────────────────────────────────────
# The mock represents valid credentials, valid schema, and valid migration
# table access (MOCK_PSQL_RESULT defaults to "t"); only the reported data
# directory differs. Identity is checked first, so the usability probe
# (the second call) must never run.

MOCK_DATA_DIRECTORY=/mock/worktree-b/data/db run_failure foreign-worktree \
  127.0.0.1 3042 crystal_forge password crystal_forge "$default_expected_dir"
assert_contains "$tmp_dir/foreign-worktree.stderr" 'different Crystal Forge database than this worktree expects'
assert_contains "$tmp_dir/foreign-worktree.stderr" "Expected data directory (this worktree): $default_expected_dir"
assert_contains "$tmp_dir/foreign-worktree.stderr" "Actual data directory (connected instance): /mock/worktree-b/data/db"
assert_line_count "$tmp_dir/foreign-worktree.psql-calls" 1

# ── Case E: identity matches, but that alone is not accepted — the
#    usability probe still runs and can still fail on its own → reject ─────
# Same final verdict as Case C, but the assertion here is architectural:
# proves the two-call sequence (identity, then usability) actually happens
# and that a passing identity check does not short-circuit success. The
# call log line count is the direct evidence: exactly 2 calls occurred, and
# only the 2nd result decided the outcome.

MOCK_PSQL_RESULT=f run_failure identity-ok-usability-fails \
  127.0.0.1 3042 crystal_forge password crystal_forge "$default_expected_dir"
assert_line_count "$tmp_dir/identity-ok-usability-fails.psql-calls" 2
assert_contains "$tmp_dir/identity-ok-usability-fails.stderr" 'cannot create in schema public'

# ── The identity query itself cannot be answered (for example, a database
#    that predates the pg_read_all_settings grant, or a connection error) →
#    fail with the underlying database error surfaced, not swallowed, and
#    without ever reaching the usability probe ──────────────────────────────

MOCK_DATA_DIRECTORY_STATUS=2 MOCK_DATA_DIRECTORY_ERROR='ERROR:  permission denied to examine "data_directory"' \
  run_failure identity-unreadable \
  127.0.0.1 3042 crystal_forge password crystal_forge "$default_expected_dir"
assert_contains "$tmp_dir/identity-unreadable.stderr" \
  'could not determine which PostgreSQL data directory role crystal_forge is using'
assert_contains "$tmp_dir/identity-unreadable.stderr" 'permission denied to examine'
assert_line_count "$tmp_dir/identity-unreadable.psql-calls" 1

# ── The connection itself fails outright (wrong password, unknown role) →
#    fail with the underlying database error surfaced, not swallowed ───────

MOCK_PSQL_STATUS=2 MOCK_PSQL_ERROR='FATAL: password authentication failed for user "crystal_forge"' \
  run_failure unreachable-db 127.0.0.1 3042 crystal_forge password crystal_forge "$default_expected_dir"
assert_contains "$tmp_dir/unreachable-db.stderr" \
  'could not determine which PostgreSQL data directory role crystal_forge is using'
assert_contains "$tmp_dir/unreachable-db.stderr" 'password authentication failed'

# ── Path normalization: a symlinked or "./"-relative expected directory
#    must compare equal to the canonical path a real PostgreSQL instance
#    reports, not fail identity purely on string formatting ────────────────

real_dir="$tmp_dir/real-data-dir"
mkdir -p "$real_dir"
link_dir="$tmp_dir/link-to-data-dir"
ln -s "$real_dir" "$link_dir"

# The caller passes the symlink path (mirroring PROJECT_ROOT resolving
# through a symlinked worktree checkout); the mock reports the real,
# already-resolved path (mirroring what a real PostgreSQL instance reports,
# since services-flake canonicalizes dataDir with `readlink -f` at
# startup). Both must normalize to the same canonical path.
MOCK_DATA_DIRECTORY="$real_dir" run_success normalized-symlink-match \
  127.0.0.1 3042 crystal_forge password crystal_forge "$link_dir"

# ── Connection details pass through unchanged, including a non-default host
#    and port (a different worktree's isolated instance, for example) ──────

MOCK_DATA_DIRECTORY=/mock/custom/data/db run_success custom-connection \
  10.1.2.3 6543 crystal_forge secret crystal_forge /mock/custom/data/db
assert_contains "$tmp_dir/custom-connection.psql-calls" "-h 10.1.2.3 -p 6543 -U crystal_forge -d crystal_forge"

printf 'db-usability-check tests passed.\n'

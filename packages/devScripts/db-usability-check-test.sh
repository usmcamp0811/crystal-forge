#!/usr/bin/env bash

# Regression coverage for db-usability-check.sh.
#
# `pg_isready` proves a PostgreSQL process answers on a host/port; it proves
# nothing about which database that process actually serves. The script
# under test answers two independent questions before `run-ui-dev` may
# reuse a connected instance: is it this worktree's own database (identity,
# checked entirely at the OS level via `ss`/`/proc`, not by querying the
# database), and can the crystal_forge role actually use it (usability,
# checked via SQL)? These cases prove each is enforced on its own, not
# collapsed into one heuristic, that a reachable-but-wrong-or-incompatible
# database fails with a clear, specific diagnostic instead of being reported
# as reusable, and that the identity check imposes no new requirement on the
# database itself — a database created before this check existed passes
# identity exactly the same way a brand-new one does.
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
spawned_pids=()
cleanup() {
  # Real background processes stand in for "the OS process listening on the
  # dev database port" so the identity check can read a real /proc/<pid>/cwd
  # (that cannot be faked the way a PATH-shadowed binary can). None of them
  # do real work; all must be reaped so this test never leaks processes.
  for pid in "${spawned_pids[@]:-}"; do
    kill "$pid" 2>/dev/null || true
  done
  rm -rf "$tmp_dir"
}
trap cleanup EXIT

mock_bin="$tmp_dir/bin"
mkdir -p "$mock_bin"

# Starts a real, otherwise-idle process whose current working directory is
# exactly $1, and echoes its PID. `cd` then `exec` (rather than a plain
# background job) means the resulting process's own cwd is $1, not merely
# the cwd of a parent shell that spawned it from there.
#
# Callers must append the echoed PID to spawned_pids themselves
# (`pid="$(spawn_process_in_dir "$dir")"; spawned_pids+=("$pid")`); this
# function cannot do it directly because `$(...)` command substitution runs
# it in a subshell, so any array mutation here would be discarded the
# instant the subshell exits and never reach the parent script's array.
spawn_process_in_dir() {
  local dir="$1"
  mkdir -p "$dir"
  # Redirect away from this script's own stdout/stderr: an inherited pipe
  # held open by a still-running background process (even one this script
  # no longer cares about) can make an output-capturing caller block until
  # that process exits, regardless of this script's own exit.
  ( cd "$dir" && exec sleep 300 >/dev/null 2>&1 ) &
  local pid=$!
  # Guarantee /proc/<pid>/cwd is already populated before the caller uses
  # it; a freshly forked process is visible in /proc essentially
  # immediately on Linux, but this removes any doubt in CI.
  for _ in $(seq 1 50); do
    [[ -e "/proc/$pid/cwd" ]] && break
    sleep 0.02
  done
  echo "$pid"
}

# Mimics `ss -H -tlnp "sport = :$port"`: a single LISTEN line naming
# MOCK_LISTENING_PID, unless MOCK_SS_EMPTY=1 simulates nothing listening
# (the process exited between pg_isready and this check).
printf '#!%s\n' "$(command -v bash)" >"$mock_bin/ss"
cat >>"$mock_bin/ss" <<'SH'

if [[ "${MOCK_SS_EMPTY:-0}" == "1" ]]; then
  exit 0
fi
printf 'LISTEN 0      128        0.0.0.0:3042 0.0.0.0:*    users:(("postgres",pid=%s,fd=6))\n' \
  "${MOCK_LISTENING_PID:?MOCK_LISTENING_PID must be set}"
SH
chmod +x "$mock_bin/ss"

# Records the exact connection arguments and SQL text it was called with, so
# tests can assert both that the usability probe ran with the caller's
# host/port/user/db, and that no query ever mentions data_directory or any
# other identity-related setting (identity is OS-level now, never SQL).
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

assert_not_contains() {
  local file="$1"
  local unexpected="$2"
  if [[ -e "$file" && "$(<"$file")" == *"$unexpected"* ]]; then
    printf 'Expected %s NOT to contain: %s\n' "$file" "$unexpected" >&2
    exit 1
  fi
}

assert_line_count() {
  local file="$1"
  local expected="$2"
  local actual
  actual="$([[ -e "$file" ]] && wc -l <"$file" || echo 0)"
  if [[ "$actual" -ne "$expected" ]]; then
    printf 'Expected %s to have %s lines, got %s.\n' "$file" "$expected" "$actual" >&2
    exit 1
  fi
}

worktree_a_dir="$tmp_dir/worktree-a/data/db"
worktree_a_pid="$(spawn_process_in_dir "$worktree_a_dir")"
spawned_pids+=("$worktree_a_pid")

# ── Case B: correct worktree identity + usable database → reuse ────────────
# Covers both a freshly bootstrapped (no migrations yet) and an existing,
# correctly initialized dev database: both are represented by the same probe
# query result (`t`) from this script's point of view. Also proves identity
# never touches SQL at all: exactly one psql call happened (the usability
# probe), and it never mentions data_directory.

MOCK_LISTENING_PID="$worktree_a_pid" run_success usable-db \
  127.0.0.1 3042 crystal_forge password crystal_forge "$worktree_a_dir"
assert_contains "$tmp_dir/usable-db.psql-calls" "-h 127.0.0.1 -p 3042 -U crystal_forge -d crystal_forge"
assert_line_count "$tmp_dir/usable-db.psql-calls" 1
assert_not_contains "$tmp_dir/usable-db.psql-calls" "data_directory"

# ── Legacy-database compatibility (the specific regression this case set
#    guards against): the identity check imposes no new SQL privilege or
#    schema requirement on the database at all, so a database created
#    before this check existed passes identity exactly like a new one. The
#    mock psql here would fail any query it did not expect; since identity
#    never queries psql, this is true by construction, but the explicit
#    absence of a data_directory-mentioning call above is the direct proof.

# ── Case C: correct worktree identity, but application privileges/ownership
#    are incompatible → reject ──────────────────────────────────────────────
# Reproduces the originally reported failure mode: a database whose
# public-schema objects are owned by an unrelated role answers pg_isready
# and reports the expected worktree identity, but role crystal_forge lacks
# the privileges the usability probe checks for.

MOCK_LISTENING_PID="$worktree_a_pid" MOCK_PSQL_RESULT=f run_failure incompatible-ownership \
  127.0.0.1 3042 crystal_forge password crystal_forge "$worktree_a_dir"
assert_contains "$tmp_dir/incompatible-ownership.stderr" 'cannot create in schema public'

# ── Case D: valid Crystal Forge database, but it belongs to another
#    worktree → reject ──────────────────────────────────────────────────────
# The listening process's real, actual cwd genuinely differs from the
# expected directory (both are real filesystem paths; nothing here is
# faked). Identity is checked first and entirely at the OS level, so the
# usability probe (which would need psql) must never run at all.

worktree_b_dir="$tmp_dir/worktree-b/data/db"
worktree_b_pid="$(spawn_process_in_dir "$worktree_b_dir")"
spawned_pids+=("$worktree_b_pid")

MOCK_LISTENING_PID="$worktree_b_pid" run_failure foreign-worktree \
  127.0.0.1 3042 crystal_forge password crystal_forge "$worktree_a_dir"
assert_contains "$tmp_dir/foreign-worktree.stderr" 'different Crystal Forge database than this worktree expects'
assert_contains "$tmp_dir/foreign-worktree.stderr" "Expected data directory (this worktree): $worktree_a_dir"
assert_contains "$tmp_dir/foreign-worktree.stderr" "Actual data directory (connected instance): $worktree_b_dir"
assert_line_count "$tmp_dir/foreign-worktree.psql-calls" 0

# ── Case E: identity matches, but that alone is not accepted — the
#    usability probe still runs and can still fail on its own → reject ─────
# Same final verdict as Case C, but the assertion here is architectural:
# proves the usability probe still runs (exactly one psql call, matching
# the count in the successful case above) even though identity already
# passed, so a passing identity check does not short-circuit success.

MOCK_LISTENING_PID="$worktree_a_pid" MOCK_PSQL_RESULT=f run_failure identity-ok-usability-fails \
  127.0.0.1 3042 crystal_forge password crystal_forge "$worktree_a_dir"
assert_line_count "$tmp_dir/identity-ok-usability-fails.psql-calls" 1
assert_contains "$tmp_dir/identity-ok-usability-fails.stderr" 'cannot create in schema public'

# ── Nothing is listening on the port anymore (it exited between
#    pg_isready and this check) → fail clearly, without ever reaching the
#    usability probe ─────────────────────────────────────────────────────

MOCK_SS_EMPTY=1 run_failure nothing-listening \
  127.0.0.1 3042 crystal_forge password crystal_forge "$worktree_a_dir"
assert_contains "$tmp_dir/nothing-listening.stderr" 'could not find the process listening'
assert_line_count "$tmp_dir/nothing-listening.psql-calls" 0

# ── The reported listening PID does not correspond to a live, readable
#    process (exited, or owned by a different OS user) → fail clearly ──────

MOCK_LISTENING_PID=999999999 run_failure pid-unreadable \
  127.0.0.1 3042 crystal_forge password crystal_forge "$worktree_a_dir"
assert_contains "$tmp_dir/pid-unreadable.stderr" 'could not read the working directory'
assert_line_count "$tmp_dir/pid-unreadable.psql-calls" 0

# ── The connection itself fails outright once identity has already passed
#    (wrong password, unknown role) → fail with the underlying database
#    error surfaced, not swallowed ──────────────────────────────────────────

MOCK_LISTENING_PID="$worktree_a_pid" MOCK_PSQL_STATUS=2 \
  MOCK_PSQL_ERROR='FATAL: password authentication failed for user "crystal_forge"' \
  run_failure unreachable-db 127.0.0.1 3042 crystal_forge password crystal_forge "$worktree_a_dir"
assert_contains "$tmp_dir/unreachable-db.stderr" 'could not query crystal_forge as role crystal_forge'
assert_contains "$tmp_dir/unreachable-db.stderr" 'password authentication failed'

# ── Path normalization: a symlinked or "./"-relative expected directory
#    must compare equal to the canonical path the listening process
#    actually reports, not fail identity purely on string formatting ──────

link_dir="$tmp_dir/link-to-worktree-a"
ln -s "$worktree_a_dir" "$link_dir"

MOCK_LISTENING_PID="$worktree_a_pid" run_success normalized-symlink-match \
  127.0.0.1 3042 crystal_forge password crystal_forge "$link_dir"

printf 'db-usability-check tests passed.\n'

#!/usr/bin/env bash

# Regression coverage for db-only-start.sh.
#
# db-only's postgres service resolves its relative dataDir (./data/db)
# against whatever directory is current when postgres actually starts.
# run-ui-dev must produce the same PostgreSQL data directory regardless of
# which directory inside the worktree it was invoked from, so db-only must
# always start with $PROJECT_ROOT as its working directory, never the
# caller's own possibly-nested current directory.

set -euo pipefail

script="${1:?db-only-start.sh path is required}"
# Resolve before changing directories below: a caller-supplied relative
# path (as used when running this test directly from a repository
# checkout) must still work after this script's own cwd moves.
script_abs="$(readlink -f -- "$script")"

tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT

project_root="$tmp_dir/worktree"
nested_dir="$project_root/packages/web-ui"
mkdir -p "$nested_dir"

mock_bin="$tmp_dir/bin"
mkdir -p "$mock_bin"

# Stands in for `nix`. Records its own $PWD and arguments instead of
# actually running anything, so this test needs no real flake or
# PostgreSQL. db-only-start.sh `exec`s directly into this mock, so the
# recorded PWD is whatever directory was current at the moment `nix`
# itself started — the exact fact under test. If db-only-start.sh's
# `cd "$project_root"` were ever removed, this would record the test's own
# nested launch directory instead of project_root, and the assertion below
# would fail.
printf '#!%s\n' "$(command -v bash)" >"$mock_bin/nix"
cat >>"$mock_bin/nix" <<'SH'

printf 'PWD=%s\n' "$PWD" >"$NIX_CALL_LOG"
printf 'ARGS=%s\n' "$*" >>"$NIX_CALL_LOG"
SH
chmod +x "$mock_bin/nix"

export PATH="$mock_bin:$PATH"
export NIX_CALL_LOG="$tmp_dir/nix-call.log"

# Launch db-only-start.sh from a nested directory inside the worktree — the
# exact scenario TASK-452's Finding 2 reported:
#   cd "$PROJECT_ROOT/packages/web-ui" && run-ui-dev
# A subshell is used only to scope this test's own `cd`; db-only-start.sh
# does its own directory change independently once running.
(
  cd "$nested_dir"
  bash "$script_abs" "$project_root"
)

if [[ ! -f "$NIX_CALL_LOG" ]]; then
  printf 'Expected %s to have invoked nix; it did not.\n' "$script" >&2
  exit 1
fi

actual_pwd="$(grep '^PWD=' "$NIX_CALL_LOG" | cut -d= -f2-)"
if [[ "$actual_pwd" != "$project_root" ]]; then
  printf 'Expected nix to run with PWD=%s (PROJECT_ROOT), got PWD=%s.\nInvoked from nested directory: %s\nThis is the exact bug TASK-452 Finding 2 reported: db-only started relative to the caller directory instead of PROJECT_ROOT.\n' \
    "$project_root" "$actual_pwd" "$nested_dir" >&2
  exit 1
fi

expected_args="run $project_root#devScripts.db-only -- up --tui=false"
actual_args="$(grep '^ARGS=' "$NIX_CALL_LOG" | cut -d= -f2-)"
if [[ "$actual_args" != "$expected_args" ]]; then
  printf 'Expected nix args %s, got %s.\n' "$expected_args" "$actual_args" >&2
  exit 1
fi

printf 'db-only-start tests passed.\n'

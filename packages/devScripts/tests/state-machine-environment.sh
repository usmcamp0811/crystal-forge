#!/usr/bin/env bash
# Gives each invocation a new disposable cluster path. The dedicated port is
# fail-closed: an existing listener is never reused. Only this launcher's
# temporary directory is removed after process-compose has stopped its children.
set -euo pipefail
PROJECT_ROOT="$(realpath "${PROJECT_ROOT:-$PWD}")"
export PROJECT_ROOT
: "${CF_STATE_MACHINE_PORT:?Dedicated test port is required}"
if pg_isready -h 127.0.0.1 -p "$CF_STATE_MACHINE_PORT" >/dev/null 2>&1; then
  echo "State-machine PostgreSQL port is occupied: $CF_STATE_MACHINE_PORT" >&2
  exit 1
fi
testRoot="$(mktemp -d "${TMPDIR:-/tmp}/cf-state-machine.XXXXXX")"
export CF_STATE_MACHINE_DATA_DIR="$testRoot/data/state-machine-db"
export CF_STATE_MACHINE_STATUS_FILE="$testRoot/test-exit-status"
# Migration-prefix copies and exact-test logs are invocation-owned too.
export TMPDIR="$testRoot/tmp"
mkdir -p "$TMPDIR"
cleanup() {
  rm -rf -- "$testRoot"
}
stop() {
  trap '' INT TERM
  kill -TERM "$composePid" 2>/dev/null || true
  wait "$composePid" || true
  exit 143
}
trap cleanup EXIT
cd "$testRoot"
"$@" &
composePid=$!
trap stop INT TERM
compose_status=0
wait "$composePid" || compose_status=$?
# Process-compose can return success after shutting down a failed child. The
# invocation-owned receipt records the test command's status independently;
# missing or malformed receipts cannot establish a passing test run.
if [[ ! -s "$CF_STATE_MACHINE_STATUS_FILE" ]]; then
  echo "State-machine tests did not produce an exit-status receipt" >&2
  if (( compose_status != 0 )); then exit "$compose_status"; fi
  exit 1
fi
read -r test_status < "$CF_STATE_MACHINE_STATUS_FILE"
if [[ ! "$test_status" =~ ^[0-9]+$ ]] || (( test_status > 255 )); then
  echo "Invalid state-machine test exit-status receipt" >&2
  exit 1
fi
if (( test_status != 0 )); then exit "$test_status"; fi
exit "$compose_status"

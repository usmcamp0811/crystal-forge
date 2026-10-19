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
cleanup() {
  rm -rf -- "$testRoot"
}
trap cleanup EXIT
export CF_STATE_MACHINE_DATA_DIR="$testRoot/data/state-machine-db"
export CF_STATE_MACHINE_SOCKET_DIR="$testRoot/data/sockets"
export CF_STATE_MACHINE_STATUS_FILE="$testRoot/test-exit-status"
# Migration-prefix copies and exact-test logs are invocation-owned too.
export TMPDIR="$testRoot/tmp"
mkdir -p "$TMPDIR"
if (( EUID == 0 )); then
  # Alpine CI supplies nobody. Resolve rather than assume its numeric IDs.
  # Only the fresh cluster parent changes owner; source and Cargo caches do not.
  export CF_STATE_MACHINE_POSTGRES_UID CF_STATE_MACHINE_POSTGRES_GID
  CF_STATE_MACHINE_POSTGRES_UID="$(id -u nobody)"
  CF_STATE_MACHINE_POSTGRES_GID="$(id -g nobody)"
  if (( CF_STATE_MACHINE_POSTGRES_UID == 0 )); then
    echo "Refusing root PostgreSQL owner" >&2
    exit 1
  fi
  mkdir -p "$testRoot/data"
  chown "$CF_STATE_MACHINE_POSTGRES_UID:$CF_STATE_MACHINE_POSTGRES_GID" "$testRoot/data"
  chmod 711 "$testRoot"
fi
stop() {
  trap '' INT TERM
  kill -TERM "$composePid" 2>/dev/null || true
  wait "$composePid" || true
  exit 143
}
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

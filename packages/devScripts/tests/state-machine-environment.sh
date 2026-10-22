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
socketRoot=""
# ShellCheck does not resolve this function's EXIT-trap invocation.
# shellcheck disable=SC2329
cleanup() {
  local status=$? cleanup_status=0
  trap - EXIT
  # Process-compose can finish before a signalled server's final checkpoint.
  # Reap the invocation's PostgreSQL explicitly before removing its files. The
  # PID file is inside our fresh data directory, never a foreign/default DB.
  local -a postgres_owner=()
  if (( EUID == 0 )) && [[ -n "${CF_STATE_MACHINE_POSTGRES_UID:-}" ]]; then
    postgres_owner=(setpriv --reuid="$CF_STATE_MACHINE_POSTGRES_UID"
      --regid="$CF_STATE_MACHINE_POSTGRES_GID" --clear-groups --no-new-privs)
  fi
  if [[ -s "$CF_STATE_MACHINE_DATA_DIR/postmaster.pid" ]] &&
    "${postgres_owner[@]}" pg_ctl -D "$CF_STATE_MACHINE_DATA_DIR" status >/dev/null 2>&1; then
    if "${postgres_owner[@]}" pg_ctl -D "$CF_STATE_MACHINE_DATA_DIR" -m fast -w stop; then
      cleanup_status=0
    else
      cleanup_status=$?
      if "${postgres_owner[@]}" pg_ctl -D "$CF_STATE_MACHINE_DATA_DIR" status >/dev/null 2>&1; then
        echo "PostgreSQL is still running; retaining invocation storage: $testRoot" >&2
        if (( status == 0 )); then status=$cleanup_status; fi
        exit "$status"
      fi
    fi
  fi
  rm -rf -- "$testRoot" || cleanup_status=$?
  if [[ -n "$socketRoot" ]]; then
    rm -rf -- "$socketRoot" || cleanup_status=$?
  fi
  # Cleanup must not hide initialization, Cargo, receipt or signal failures.
  if (( status == 0 )); then status=$cleanup_status; fi
  exit "$status"
}
trap cleanup EXIT
export CF_STATE_MACHINE_DATA_DIR="$testRoot/data/state-machine-db"
# PostgreSQL appends an initialization suffix and socket filename. Linux limits
# the complete Unix socket path to 107 bytes; TMPDIR can already exceed that.
# Keep sockets in a separate short, invocation-owned directory and clean both.
socketRoot="$(mktemp -d /tmp/cf-sm-sockets.XXXXXX)"
export CF_STATE_MACHINE_SOCKET_DIR="$socketRoot"
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
  chown "$CF_STATE_MACHINE_POSTGRES_UID:$CF_STATE_MACHINE_POSTGRES_GID" "$socketRoot"
  chmod 711 "$testRoot"
fi
printf 'Database invocation: runner_uid=%s data_root=%s socket_root=%s\n' \
  "$EUID" "$testRoot" "$socketRoot"
# ShellCheck does not resolve this function's INT/TERM-trap invocation.
# shellcheck disable=SC2329
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
printf 'Database completion: test_exit=%s compose_exit=%s\n' "$test_status" "$compose_status"
if (( test_status != 0 )); then exit "$test_status"; fi
exit "$compose_status"

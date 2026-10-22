#!/usr/bin/env bash
# Runs inside nix develop #sqlx. Reset the shell's development URL before any
# database operation. Check the server's actual path and privileges before
# migrations; a racing foreign listener must never become a test database.
set -euo pipefail
# The SQLx development shell exports example application settings, including a
# legacy map-shaped flakes.watched value. They are not test inputs and can
# override a scoped worker fixture or point it at development services. Remove
# their names without printing values before configuring this isolated run.
# The SQLx shell's Bash does not provide compgen. A failed command inside
# process substitution does not fail the consuming loop, so compgen would leave
# all development overrides intact. Enumerate exported names with coreutils;
# NUL delimiters preserve values with newlines without printing any values.
while IFS='=' read -r -d '' variable _; do
  case "$variable" in CRYSTAL_FORGE__*) unset "$variable" ;; esac
done < <(env -0)
: "${CF_STATE_MACHINE_DATA_DIR:?Use devScripts.state-machine-test}"
: "${CF_STATE_MACHINE_PORT:?Use devScripts.state-machine-test}"
export PGHOST=127.0.0.1 PGPORT="$CF_STATE_MACHINE_PORT"
export PGUSER=crystal_forge PGDATABASE=crystal_forge
unset PGPASSWORD PGSERVICE PGSERVICEFILE PGOPTIONS
export DATABASE_URL="postgresql://$PGUSER@$PGHOST:$PGPORT/$PGDATABASE"
export CRYSTAL_FORGE_TEST_DATABASE_URL="$DATABASE_URL"
actualDataDir="$(psql -XAt -v ON_ERROR_STOP=1 -c 'SHOW data_directory')"
if [[ "$actualDataDir" != "$CF_STATE_MACHINE_DATA_DIR" ]]; then
  echo "Refusing state-machine tests: PostgreSQL data directory mismatch" >&2
  exit 1
fi
if [[ "$(psql -XAt -v ON_ERROR_STOP=1 -c "SELECT rolcanlogin AND rolsuper AND rolcreatedb FROM pg_roles WHERE rolname=current_user")" != t ]]; then
  echo "State-machine role requires LOGIN SUPERUSER CREATEDB" >&2
  exit 1
fi
if [[ "$(psql -XAt -v ON_ERROR_STOP=1 -c 'SHOW shared_preload_libraries')" != pg_stat_statements ]]; then
  echo "State-machine PostgreSQL requires pg_stat_statements preload" >&2
  exit 1
fi
export SQLX_OFFLINE=true CARGO_NET_OFFLINE=true
# All targets and exact selections share this compilation cache. It is separate
# from ordinary package builds, whose feature/profile settings can differ.
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$PROJECT_ROOT/packages/default/target/database-tests}"
cd "$PROJECT_ROOT/packages/default"

# Cargo/libtest returns zero for a stale filter or an ignored-only selection.
# Require a positive executed count for every test invocation, including broad
# state-machine groups. Compilation and SQLx commands have no libtest receipt.
# Preserve Cargo's real failure through tee and keep repeated rustc diagnostics
# out of CI's bounded log unless the command fails. Nested exact-test shells
# inherit this same guard; there is no second test execution for counting.
cargo() {
  local output errors status=0 passed=0 failed=0 ignored=0 line argument execution=false
  if [[ "${1:-}" == test ]]; then
    execution=true
    for argument in "$@"; do
      case "$argument" in --no-run|--list) execution=false ;; esac
    done
  fi
  output="$(mktemp "$TMPDIR/cargo-stdout.XXXXXX")"
  errors="$(mktemp "$TMPDIR/cargo-stderr.XXXXXX")"
  if (set -o pipefail; command cargo "$@" 2>"$errors" | tee "$output"); then
    status=0
  else
    status=$?
  fi
  if [[ "$execution" == true ]]; then
    while IFS= read -r line; do
      if [[ "$line" =~ ^test\ result:\ (ok|FAILED)\.\ ([0-9]+)\ passed\;\ ([0-9]+)\ failed\;\ ([0-9]+)\ ignored\; ]]; then
        passed=$((passed + BASH_REMATCH[2]))
        failed=$((failed + BASH_REMATCH[3]))
        ignored=$((ignored + BASH_REMATCH[4]))
      fi
    done < "$output"
    if (( status == 0 && passed == 0 )); then
      echo "Database test selection executed no successful tests: $*" >&2
      status=1
    fi
    printf 'DATABASE_SELECTION passed=%s failed=%s ignored=%s exit=%s command=cargo' \
      "$passed" "$failed" "$ignored" "$status"
    printf ' %q' "$@"
    printf '\n'
  fi
  if (( status != 0 )); then
    tail -n 400 "$errors" >&2
  fi
  rm -f -- "$output" "$errors"
  return "$status"
}
export -f cargo

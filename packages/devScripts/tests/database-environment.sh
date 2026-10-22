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

# Bind execution to libtest's selected names, not just a positive result count.
# Listing uses the same Cargo targets/filters and libtest options, and never
# executes test bodies. Default --list includes ignored tests: subtract the
# ignored-only listing unless execution explicitly enables those tests. Compare
# sorted multisets so repeated names in different workspace targets retain
# their multiplicity. Nested exact-test shells inherit this same guard.
cargo() {
  local work output errors status=0 cleanup_status=0 passed=0 failed=0 ignored=0
  local line name argument execution=false separator=false mode=default
  local expected_count=0 actual_count=0 named_passed=0 named_failed=0
  local -a list_args=("$@")
  if [[ "${1:-}" == test ]]; then
    execution=true
    for argument in "$@"; do
      case "$argument" in
        --) separator=true ;;
        --no-run|--list) execution=false ;;
        --ignored) mode=ignored ;;
        --include-ignored) mode=all ;;
      esac
    done
  fi
  work="$(mktemp -d "$TMPDIR/cargo-selection.XXXXXX")" || return $?
  output="$work/stdout"
  errors="$work/stderr"
  if [[ "$execution" == true ]]; then
    if [[ "$separator" == false ]]; then list_args+=(--); fi
    list_args+=(--list)
    if command cargo "${list_args[@]}" >"$work/list" 2>>"$errors"; then
      : > "$work/names"
      while IFS= read -r line; do
        if [[ "$line" =~ ^(.+):\ test$ ]]; then
          printf '%s\n' "${BASH_REMATCH[1]}" >> "$work/names"
        fi
      done < "$work/list"
      LC_ALL=C sort "$work/names" > "$work/expected"
      if [[ "$mode" == default ]]; then
        if command cargo "${list_args[@]}" --ignored >"$work/ignored-list" 2>>"$errors"; then
          : > "$work/ignored-names"
          while IFS= read -r line; do
            if [[ "$line" =~ ^(.+):\ test$ ]]; then
              printf '%s\n' "${BASH_REMATCH[1]}" >> "$work/ignored-names"
            fi
          done < "$work/ignored-list"
          LC_ALL=C sort "$work/ignored-names" > "$work/ignored"
          LC_ALL=C comm -23 "$work/expected" "$work/ignored" > "$work/selected"
          mv "$work/selected" "$work/expected"
        else
          status=$?
        fi
      fi
      while IFS= read -r line; do expected_count=$((expected_count + 1)); done < "$work/expected"
      if (( status == 0 && expected_count == 0 )); then
        echo "Database test selection lists no executable tests: $*" >&2
        status=1
      fi
    else
      status=$?
    fi
    if (( status != 0 )); then echo "Database test selection listing failed" >&2; fi
  fi
  # Execute each selection once. A listing/setup failure must not run tests.
  if (( status == 0 )); then
    if (set -o pipefail; command cargo "$@" 2>>"$errors" | tee "$output"); then
      status=0
    else
      status=$?
    fi
  fi
  if [[ "$execution" == true && -f "$output" ]]; then
    : > "$work/actual-names"
    while IFS= read -r line; do
      if [[ "$line" =~ ^test\ result:\ (ok|FAILED)\.\ ([0-9]+)\ passed\;\ ([0-9]+)\ failed\;\ ([0-9]+)\ ignored\; ]]; then
        passed=$((passed + BASH_REMATCH[2]))
        failed=$((failed + BASH_REMATCH[3]))
        ignored=$((ignored + BASH_REMATCH[4]))
      fi
      if [[ "$line" =~ ^test\ (.+)\ \.\.\.\ (ok|FAILED)$ ]]; then
        # Pretty libtest output annotates expected panics; --list reports the
        # original Rust name without that display-only suffix.
        name="${BASH_REMATCH[1]% - should panic}"
        printf '%s\n' "$name" >> "$work/actual-names"
        actual_count=$((actual_count + 1))
        if [[ "${BASH_REMATCH[2]}" == ok ]]; then
          named_passed=$((named_passed + 1))
        else
          named_failed=$((named_failed + 1))
        fi
      fi
    done < "$output"
    if (( status == 0 && passed == 0 )); then
      echo "Database test selection executed no successful tests: $*" >&2
      status=1
    fi
    LC_ALL=C sort "$work/actual-names" > "$work/actual"
    if ! cmp -s "$work/expected" "$work/actual" ||
      (( named_passed != passed || named_failed != failed )); then
      echo "Database test selection membership mismatch" >&2
      echo "Missing listed names:" >&2
      LC_ALL=C comm -23 "$work/expected" "$work/actual" >&2
      echo "Unexpected emitted names:" >&2
      LC_ALL=C comm -13 "$work/expected" "$work/actual" >&2
      if (( status == 0 )); then status=1; fi
    fi
    if (( status == 0 && failed != 0 )); then status=1; fi
    printf 'DATABASE_MEMBERSHIP expected=%s emitted=%s exit=%s\n' \
      "$expected_count" "$actual_count" "$status"
    printf 'DATABASE_SELECTION passed=%s failed=%s ignored=%s exit=%s command=cargo' \
      "$passed" "$failed" "$ignored" "$status"
    printf ' %q' "$@"
    printf '\n'
  fi
  if (( status != 0 )); then
    tail -n 400 "$errors" >&2
  fi
  rm -rf -- "$work" || cleanup_status=$?
  if (( status == 0 )); then status=$cleanup_status; fi
  return "$status"
}
export -f cargo

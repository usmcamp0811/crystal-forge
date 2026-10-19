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

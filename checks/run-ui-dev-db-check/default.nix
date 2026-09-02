# Regression check for db-usability-check.sh, the probe `run-ui-dev` runs
# before trusting an already-running PostgreSQL instance as its dev
# database.
#
# `pg_isready` only proves a PostgreSQL process answers on a port; it does
# not prove the crystal_forge role can use the crystal_forge database there.
# This check proves the probe script tells those two things apart instead of
# letting `run-ui-dev` start the server against an unusable database. It
# starts no real PostgreSQL instance; `psql` is mocked.
{ pkgs, ... }:
pkgs.runCommand "run-ui-dev-db-check"
{
  nativeBuildInputs = with pkgs; [ bash coreutils ];
} ''
  bash ${../../packages/devScripts/db-usability-check-test.sh} \
    ${../../packages/devScripts/db-usability-check.sh}
  touch "$out"
''

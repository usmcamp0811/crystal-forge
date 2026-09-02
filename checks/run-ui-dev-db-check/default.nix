# Regression check for run-ui-dev's database-related behavior:
#
# 1. db-usability-check.sh — the probe `run-ui-dev` runs before trusting
#    an already-running PostgreSQL instance as its dev database. `pg_isready`
#    only proves a PostgreSQL process answers on a port; it does not prove
#    the crystal_forge role can use the crystal_forge database there, or
#    that the process belongs to this worktree at all. This test proves the
#    probe script tells those things apart instead of letting `run-ui-dev`
#    start the server against an unusable or foreign database. It starts no
#    real PostgreSQL instance (`psql` is mocked), but does exercise the real
#    `ss`/`/proc` based worktree-identity lookup against real background
#    processes the test spawns itself.
#
# 2. db-only-start-test.sh — proves `db-only-start.sh` starts db-only with
#    $PROJECT_ROOT as its working directory, not the (possibly nested)
#    directory run-ui-dev was invoked from. Without this, the relative
#    dataDir (./data/db) resolves to a different location than the
#    worktree-identity check expects, silently diverging.
{ pkgs, ... }:
pkgs.runCommand "run-ui-dev-db-check"
{
  nativeBuildInputs = with pkgs; [ bash coreutils iproute2 ];
} ''
  bash ${../../packages/devScripts/db-usability-check-test.sh} \
    ${../../packages/devScripts/db-usability-check.sh}
  bash ${../../packages/devScripts/db-only-start-test.sh} \
    ${../../packages/devScripts/db-only-start.sh}
  touch "$out"
''

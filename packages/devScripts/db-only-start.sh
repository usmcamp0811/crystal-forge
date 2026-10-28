#!/usr/bin/env bash

# Starts db-only (this repository's PostgreSQL-only dev-stack service) with
# the given PROJECT_ROOT as its working directory. The caller is expected
# to background and `setsid` this invocation itself (see run-ui-dev); this
# script only owns getting the working directory right before handing off
# to `nix run`.
#
# db-only's postgres service uses a relative dataDir (./data/db — see
# dbOnly.config.services.postgres.db.dataDir in default.nix), which
# services-flake resolves against whatever directory is current when
# postgres actually starts, not this repository's root. `nix run` does not
# change directory on its own, so without the explicit `cd` below, invoking
# run-ui-dev from inside a subdirectory (packages/web-ui, for example)
# would bootstrap postgres at that subdirectory's own ./data/db instead of
# $PROJECT_ROOT/data/db, silently diverging from the worktree-identity
# check in db-usability-check.sh, which always expects the latter.
#
# `exec` replaces this process with `nix run` in place (same PID, same
# session/process group the caller established), so the caller's PID and
# process-group bookkeeping is unaffected; this script never becomes a
# lingering disposable shell process sitting above the real one.

set -euo pipefail

project_root="${1:?usage: db-only-start.sh PROJECT_ROOT}"

cd "$project_root"
exec nix run "$project_root#devScripts.db-only" -- up --tui=false

# devenv Parallel-Worktree Development Workflow

TASK-462.1 (Phase 1) adds an additive, devenv-based reproduction of
`run-ui-dev` (PostgreSQL, the Crystal Forge API server, and the Dioxus web
UI dev server) with per-worktree isolation and dynamically allocated
ports. Two Git worktrees can run this stack at the same time with no
manual port coordination.

This workflow does **not** replace, remove, or modify:

- `nix develop` (the existing default shell, `shells/default/default.nix`)
- `run-ui-dev`, `db-only`, or any other existing shell alias
- `process-compose-flake` / `services-flake` profiles
- `db-usability-check.sh` or `db-only-start.sh`
- The authoritative NixOS `web-ui` check

All of the above continue to work exactly as before. TASK-462.2 tracks
migrating the remaining process-compose profiles and retiring redundant
legacy scripts once devenv parity is proven for them; this task migrates
only the `run-ui-dev` workflow.

## Why this is not `devenv.lib.mkShell`

devenv supports two integration styles: flake integration
(`devenv.lib.mkShell`, evaluated through plain `nix build`/`nix develop`)
and devenv's native project format (`devenv.yaml` + `devenv.nix`, driven
by the real `devenv` CLI binary). This repository uses the native format.

That is a correctness choice, not a style preference.
`processes.<name>.ports.<port>.value` is only resolved to a genuinely free
port when devenv's own compiled Nix backend evaluates the module (its
`allocatePort` primop). Flake integration evaluates the same module
through plain Nix, which never has that primop available, so the
"resolved" port silently equals the requested base, unchanged, even when
that port is already bound elsewhere. This was verified empirically while
implementing this task: occupying a base port and evaluating
`processes.<name>.ports.<port>.value` through `devenv.lib.mkShell` still
returned the occupied base port, while the same module evaluated through
the native `devenv` CLI correctly skipped to the next free port.

`flake.nix`'s additive `devShells.devenv` output exists only to put the
real `devenv` CLI on `PATH`; it does not itself evaluate `devenv.nix`.

## Starting the stack

```bash
nix develop .#devenv     # puts the real `devenv` CLI on PATH
devenv up                 # foreground: PostgreSQL, API server, web UI dev server
```

Or detached:

```bash
devenv up -d
devenv processes list      # resolved ports and process status
devenv processes logs api  # per-process logs (also: postgres, web)
devenv down                 # stop only this worktree's stack
```

`devenv up`/`devenv shell` work from any shell that has the real `devenv`
CLI on `PATH` (`nix develop .#devenv` is the additive, repository-provided
way to get one); they read `devenv.yaml`/`devenv.nix` directly and do not
require entering `nix develop .#devenv` first if `devenv` is already
available some other way.

Run these from this worktree's root. `devenv.root` (and therefore all
per-worktree state below) is derived from the current working directory.

## What gets isolated, and how

| Concern | Mechanism |
| --- | --- |
| PostgreSQL port | `services.postgres` + devenv automatic port allocation (`processes.postgres.ports.main.allocate`), base `5432` |
| PostgreSQL data directory | `$DEVENV_STATE/postgres`, under `<worktree-root>/.devenv/state` |
| API server port | devenv automatic port allocation, base `3445` |
| Web UI dev server port | devenv automatic port allocation, base `8080` |
| Generated `crystal-forge-config.toml` | `$DEVENV_STATE/crystal-forge-config.toml`, resolved ports baked in |
| Portless project identity | devenv's own fallback to the worktree directory basename (see below) |

Every one of these is keyed off `config.devenv.root`, the absolute path of
the worktree devenv was run from, so two worktrees never share state, a
data directory, a generated config file, or (in practice) a port.

### PostgreSQL: TCP, not a Unix socket

A per-worktree Unix socket was tried first: it sidesteps port allocation
(and Portless) entirely, since `services.postgres` allocates no port at
all when `listen_addresses = ""`. It does not integrate with the actual
Crystal Forge server, though: `crates/cf-server/src/config/database.rs`
builds its connection string as a plain
`postgres://{user}:{password}@{host}:{port}/{name}` URL, which cannot
represent a Unix socket directory as `host` (verified empirically: the
server failed every connection with `both host and hostaddr are
missing`). This module uses a dynamically allocated TCP port
(`services.postgres.listen_addresses = "127.0.0.1"`) instead, exactly
like the API and web UI ports.

### Worktree identity for Portless

devenv's own hostname resolution (`project_name` in the upstream `devenv`
CLI) falls back to the basename of `devenv.root` whenever the `name`
option is left at its module default (`"devenv-shell"`). `devenv.nix`
deliberately leaves `name` unset so this fallback applies: every worktree
following `docs/agents/worktrees.md`'s `TASK-ID-short-slug` convention
gets a distinct, stable Portless hostname automatically, with no bespoke
Nix code, and no risk of two worktrees colliding on the same hostname.

### Dynamic-port env vars can go stale between separate CLI invocations

Nix-evaluation-time values (`config.env.CF_UI_DEV_API_BASE_URL`, etc.) are
devenv's best guess at evaluation time, using the same port-allocation
primop `devenv up`'s native process manager uses — but a *separate* CLI
invocation is not guaranteed to reevaluate that primop identically to
whichever invocation is actually managing the running processes. This was
observed while implementing this task: a `devenv shell` run independently
of an already-running `devenv up` reported a different, stale port than
the one the running API server actually bound.

`enterShell` works around this: whenever a process manager is already
running for this worktree, it re-derives `CF_UI_DEV_API_BASE_URL`,
`CF_UI_DEV_BASE_URL`, and `DB_PORT` from `devenv processes list`'s live
output (querying the running manager directly, which is authoritative)
instead of trusting the static Nix-evaluation-time guess. Re-enter the
shell (`devenv shell`) after `devenv up` to pick up the actual resolved
values, or run `devenv processes list` directly at any time.

## Discovering resolved ports and hostnames

```bash
devenv processes list
```

```
api                            ready restarts: 0 ports: http:3447
postgres                       ready restarts: 0 ports: main:5432
web                            ready restarts: 0 ports: http:8081
```

Entering `devenv shell` (or `nix develop .#devenv` then `devenv shell`,
or simply starting `devenv up`) also prints a banner with the same
information, plus the current Portless URLs when enabled.

## Running `web-ui-test` against this stack

`checks/web-ui/tests/web-ui-test.sh` is unmodified. `devenv.nix` exports
the exact environment variables it already reads as overrides:

```bash
devenv shell
web-ui-test 12-systems
```

`web-ui-test` (matching `checks/web-ui/tests/web-ui-test.sh`'s reads) is on
`PATH` directly inside `devenv shell`, the same script
`shells/default/default.nix`'s legacy `nix run .#devScripts.webUiTest`
alias resolves to. `CF_UI_DEV_BASE_URL` / `CF_UI_DEV_API_BASE_URL` point
it at this worktree's resolved UI/API ports; `DB_HOST` / `DB_PORT` /
`DB_USER` / `DB_PASSWORD` / `DB_NAME` are what it falls back to for its
fixture-backed workflows' direct PostgreSQL precondition check.

## Portless: opt-in `.localhost` URLs

Portless is disabled by default (`process.proxy.enable = lib.mkDefault
false;`). On Linux, enabling it makes devenv ask for sudo authentication
to bind the shared proxy's port-80 listener; nothing in this stack depends
on that succeeding, and every workflow above (including `web-ui-test`)
works identically with Portless disabled — this is, in practice, the
normal path.

To try it, override `process.proxy.enable` locally (do not commit a
change to the default) and restart the stack:

```bash
devenv shell -O process.proxy.enable:bool true
devenv up
```

With Portless enabled, the API and web UI processes get stable
`http://api.<worktree-identity>.localhost` and
`http://web.<worktree-identity>.localhost` URLs (`<worktree-identity>` is
this worktree's directory basename, sanitized by devenv). PostgreSQL is
never proxied: `services.postgres`'s automatic port allocation is only
active when `listen_addresses` is non-empty, which is this module's
mechanism (see above), so PostgreSQL's process does get an allocated port
and devenv's Portless route builder does generate a route for it, since
`services.postgres` provides no way to opt a process out of route
generation. That route is inert: PostgreSQL does not speak HTTP, so it can
never actually carry PostgreSQL traffic through the shared HTTP proxy.
Ignore it.

## Relationship to the legacy workflow

| | Legacy (`nix develop` / `run-ui-dev`) | devenv (`nix develop .#devenv`) |
| --- | --- | --- |
| PostgreSQL port | fixed `3042` | dynamically allocated, base `5432` |
| API server port | fixed `3445` | dynamically allocated, base `3445` |
| Web UI port | fixed `8080` | dynamically allocated, base `8080` |
| Two worktrees at once | requires `db-usability-check.sh` and manual coordination | isolated automatically |
| Portless `.localhost` URLs | not available | opt-in |

Both workflows currently coexist. Use `run-ui-dev` if you want the
existing fixed-port behavior (for example, to match a script or bookmark
that assumes `3445`/`8080`); use the devenv workflow to run more than one
worktree's stack at the same time. TASK-462.2 covers migrating the
remaining process-compose profiles.

## Two narrow, backward-compatible application changes

Two small pieces of the application itself assumed the fixed dev ports
below and needed a minimal, backward-compatible extension point:

- `packages/web-ui/src/api/client.rs`'s `backend_origin_for_dev` (the
  WASM app's own dev-mode API-origin detection) now also accepts a
  `CF_UI_DEV_API_PORT` compile-time environment variable. Unset in every
  other build (including `run-ui-dev`'s), so behavior there is unchanged.
- `crates/cf-server/src/bin/server.rs`'s CORS layer (a fixed allowlist of
  `8080`/`8081`/`8000`) now also accepts a `CRYSTAL_FORGE_CORS_DEV_ORIGIN`
  runtime environment variable naming one additional allowed origin.
  Unset in every other workflow, so the fixed allowlist there is
  unchanged.

`devenv.nix` sets both to this worktree's own resolved values.

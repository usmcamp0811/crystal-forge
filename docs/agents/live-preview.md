# Live preview during UI development

## Required outcome

The user must be able to inspect the running Dioxus application while an agent
changes it. Start the preview before UI edits. Publish the URL when the application
is usable. Keep it current through implementation and leave it available at
handoff. Do not wait for a commit, an MR, deployment, or broad verification.

This applies to views, components, styling, browser interactions, and backend
changes made as part of a browser-visible workflow. Read-only analysis and work
with no browser-visible effect do not require a preview. An existing runtime
limitation must be reported, not hidden behind an old successful browser build.

The task owner is responsible for the preview. Delegation does not remove that
responsibility. One task may share its preview among cooperating subagents, but
only the owner controls its processes and data. Separate task worktrees need
separate verified environments; they must not share mutable preview databases.

This policy changes the feedback loop only. Existing task, worktree, branch,
merge-approval, database, and final-review rules remain in force. Starting a local
preview is not authorization to deploy to the user's long-running instance.

## Inspect the launcher in the active worktree

Use the repository's Nix environment and current development tooling. Inspect
these paths before the first startup, and again if the task changes them:

- `packages/devScripts/default.nix`: `runUiDev` and `runUiFrontend`.
- `shells/default/default.nix`: shell initialization and helper aliases.
- `packages/devScripts/db-only-start.sh` and `db-usability-check.sh`.
- `packages/web-ui/Dioxus.toml` and `src/api/client.rs`.

The implementation inspected at `a3e4fac731d541b521844f1560c63d4ac5e6fd98`
has the following constraints. Recheck them on newer branches.

| Component | Existing behavior |
| --- | --- |
| `run-ui-dev` | Starts or checks the worktree database, starts a fixture-backed backend, then runs `dx serve --platform web`. |
| Default endpoints | Browser `http://localhost:8080`; API `http://127.0.0.1:3445`; PostgreSQL `127.0.0.1:3042`. |
| Frontend source | Dioxus reads `packages/web-ui` in the working tree. |
| Backend | Starts once. It is not restarted by the frontend watcher. The default uses the core backend package; `--dev` uses the full `#server` package. |
| Toolchain | Launcher checks Dioxus CLI `0.7.3` and installs the `0.2.108` wasm-bindgen link. Preserve these pinned helpers. |
| Tailwind | Generated once on startup, not watched by these launchers. |
| API routing | `backend_origin_for_dev()` recognizes `8080`, `8000`, and `8081`, reads `cf_backend_origin` from local storage, then defaults to port `3445`. |
| Configuration | The launcher can reuse `CRYSTAL_FORGE_CONFIG`. Shell initialization also generates configuration. Inspect the final effective configuration and environment overrides. |
| Isolation | The database guard checks worktree ownership. Ports, some configuration/key locations, and the database log path are shared defaults. They are not a complete multi-preview isolation mechanism. |

The comment in `Dioxus.toml` that names backend port `3000` is stale at this
revision. Use the implementation and observed browser requests, not that comment.
Do not assume Dioxus proxies the API.

## Verify safety before startup

Confirm the task root and branch. Enter `nix develop` from that root. Confirm
`PROJECT_ROOT` again after shell initialization; an inherited value or a shell
started in a subdirectory must not select another tree.

Before starting a server, applying migrations, or seeding fixtures, verify the
actual database target and its ownership. Check the effective application config,
`DATABASE_URL`, relevant `CRYSTAL_FORGE__DATABASE__*` overrides, process identity,
and data directory. Keep passwords and other secrets out of reports.

Use only the task's preview database. Do not use the user's long-running database,
another task's database, a shared integration database, or port `5432` by default.
A successful `pg_isready` call, a local address, or a name containing `dev` is not
proof of isolation. Do not bypass the repository's worktree database guard.

Verify that the API listener belongs to this task too. A successful `/status`
response can come from an old or unrelated server. Check the process, executable,
configuration, and source revision before calling the preview current.

Check the browser's API origin before login or mutation. A saved
`cf_backend_origin` can redirect the frontend to a different backend. Correct only
the task preview's origin setting. Do not clear the user's unrelated browser data.
Use a separate browser profile/context for automation and avoid reusing real
instance credentials or sessions.

Use fixture data and mock execution for ordinary UI work. Verify that the preview
cannot deploy to real systems, claim real builder work, write real caches, or send
real notifications. Mock mode alone is not a network security boundary. Do not
copy production credentials or register real workers for preview convenience.
A historical-data copy requires separate explicit authorization and isolation.

Prefer loopback access. The existing stack includes non-loopback listener
defaults; verify exposure rather than assuming the stack is private. Do not open
firewall ports or expose fixture credentials to the network without permission.

## Start and retain the preview

After the safety checks, the existing interactive entry point is:

```bash
# Run from the task worktree root.
nix develop
export PROJECT_ROOT="$(git rev-parse --show-toplevel)"
run-ui-dev
```

`run-ui-dev` is a shell alias. In an automation shell where aliases do not expand,
use the existing flake entry point from the same verified Nix environment:

```bash
nix run "$PROJECT_ROOT#devScripts.runUiDev"
```

Keep that command in a persistent terminal or task-owned process supervisor. Use
the agent runner's persistent session facility when available. Otherwise use an
available Nix-provided supervisor. A short-lived command followed by `&` is not
proof that the preview will survive the next agent command or session boundary.

Record the session name, process identities, worktree, ports, configuration path,
database identity, and logs. Keep runtime files outside tracked source or in an
existing ignored location. Do not commit logs, credentials, PIDs, or demo data.
Use task-specific logs when supported. Do not overwrite another task's shared log.

A frontend-only restart may use `run-ui-frontend` after the owner verifies that
the intended task backend is still running and current. Do not use it to attach
to a convenient but unrelated server.

Wait for the first frontend build. Verify login, an API-backed page, the affected
route, stylesheet loading, and the hot-reload connection. Check console/network
errors. Then report the browser URL immediately and continue the task without
waiting for the user to approve each routine edit.

For work on another host, report that host and the required forwarding setup.
Do not describe remote loopback as the user's localhost. Account for the API port
and reload WebSocket as well as the frontend; forwarding only the frontend may
leave browser API requests pointing at the wrong machine.

## Keep the preview current

Work in small runnable increments. Check the affected view after each coherent
change. RSX/style updates and Rust recompilation have different update paths;
verify what the browser actually received instead of promising instant reload for
every edit. Report rebuilding or stale state when the last successful bundle is
still displayed.

Keep generated styles current. Start a task-owned watcher with the pinned
Tailwind tool, or regenerate the stylesheet after changes that introduce classes.
The current input is `packages/web-ui/tailwind.css`; the output is
`packages/web-ui/assets/tailwind.min.css`. Verify asset delivery for `app.css`,
Tailwind, and other changed assets. Do not assume `watch_path = ["src"]` proves
that every stylesheet is watched.

When backend code changes, rebuild it from this worktree and restart only the
task-owned backend. Verify the new process and changed API behavior. Merely
restarting Dioxus or refreshing the browser does not replace the running server.
Stage only intended new source files when a Git-backed Nix build needs them;
do not commit early just to make files visible to Nix.

When a migration changes, apply it only to the verified preview database. Do not
edit an already-applied migration or reset a database to hide an upgrade failure.
Preserve the user's current preview data and route where practical. Announce any
necessary restart or reseed before disrupting active review.

Prepare representative local records for the affected view. A populated-state
change needs populated data. Check empty, loading, error, and restricted-access
states when affected. Compare the actual implementation with the task's accepted
design, including omitted sections, ordering, metadata, interactions, and visual
hierarchy. A design mock is a reference, not a replacement for the application.

Do not drive automation in the user's active browser tab. Tests that mutate,
reset, or reseed data must use separate test state, not the database the user is
currently browsing. Do not change production behavior to make a preview look
successful. Label mock or incomplete backend behavior explicitly.

## Parallel tasks and startup failures

The inspected launcher uses fixed ports. Do not invent `UI_PORT`, `API_PORT`, or
`DB_PORT` overrides and assume the launcher honors them. Changing only the Dioxus
port does not isolate API routing, PostgreSQL, process-compose control endpoints,
auxiliary services, keys, configuration, or logs.

If another task owns a required resource, do not kill it or reuse its database.
Use an already-supported isolated arrangement, such as another development host,
or report the precise tooling limitation. Do not change committed application
ports or networking solely to hide a collision. A reusable multi-preview launcher
change needs its own approved scope unless already included in the task.

For a broken build, unavailable tool, database ownership failure, or missing
browser access, report the actual condition promptly. Repair the bounded startup
problem within scope or continue independent non-UI work. Do not silently finish
the UI without a usable preview. Only the user can waive the requirement. Do not
run the full browser VM repeatedly as a substitute for fixing preview startup.

If the environment cannot retain processes across agent turns, say so. Never
claim that an unreachable or terminated session remains available.

## Verification and handoff

Use the preview for immediate feedback and focused tests for correctness. Do not
run the full `web-ui` VM suite or `nix flake check` just to preview an edit. Required
authoritative browser checks and MR screenshots still apply before final review;
use exact-head CI when the task permits it. Preview screenshots do not count as
an authoritative check result. A user's quick visual inspection does not prove
untested authorization, persistence, concurrency, or end-to-end behavior.

Include this information when the preview first becomes ready and at handoff:

```text
Preview: ready | rebuilding | blocked | stopped
Browser URL: <verified address, or unavailable>
Review path: <route and steps to open the affected state>
Source: <branch, HEAD, absolute worktree; note uncommitted edits>
Backend: <build/process identity; current or stale>
Data: <fixture-backed or explicitly authorized copy; mock behavior>
Login: <verified local fixture login instructions; no real credentials>
Session/logs: <task-owned supervisor/session and log paths>
Known gaps: <what is not working or not verified>
```

Repeat the URL in meaningful progress updates. Do not repeat a long startup report
after every edit. Keep these preview health labels separate from backlog statuses.

Leave the preview running at handoff unless the user requests shutdown or the
task is being retired. Before worktree removal, stop only the owned processes and
preserve data according to the task's cleanup authority. Do not assume one Ctrl+C
stopped the backend, database, and watchers. Verify actual ownership and shutdown;
never use broad `pkill`, port-based kills, or a database reset as routine cleanup.

At takeover, the next owner must verify and resume the recorded session or report
why it is unavailable. Do not start a second stack or reset the database merely
because the agent context changed.

---
id: TASK-462.1
title: >-
  Phase 1: Add a devenv shell with dynamic-port processes and prove
  parallel-worktree isolation for run-ui-dev
status: In Progress
assignee: []
created_date: '2026-09-19 14:51'
updated_date: '2026-09-20 18:03'
labels: []
milestone: m-1
dependencies: []
references:
  - shells/default/default.nix
  - packages/devScripts/default.nix
  - packages/devScripts/db-only-start.sh
  - packages/devScripts/db-usability-check.sh
  - checks/web-ui/tests/web-ui-test.sh
  - docs/agents/worktrees.md
  - docs/agents/database-safety.md
  - docs/agents/verification.md
parent_task_id: TASK-462
priority: high
type: enhancement
ordinal: 479000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Context

Read TASK-462 for the full problem statement, scope boundary, and constraints. This subtask is Phase 1: the minimum change that proves the devenv migration is viable, without touching the existing `nix develop` shell or any `process-compose-flake` profile other than by addition.

Today, `run-ui-dev` (`packages/devScripts/default.nix`) starts PostgreSQL on the fixed port `3042` via the `db-only` process-compose profile, waits for `pg_isready`, then runs `db-usability-check.sh` to detect whether the process answering on that port actually belongs to this worktree (see the extensive comment above that call site explaining why this check exists: every worktree shares the fixed port `3042` but has its own on-disk data directory, so a different worktree's leftover PostgreSQL process, or a database owned by an unrelated role, can answer instead). The API server binds `3445` and the Dioxus dev server binds `8080`, both fixed. `checks/web-ui/tests/web-ui-test.sh` defaults to those same two fixed addresses via `CF_UI_DEV_BASE_URL` / `CF_UI_DEV_API_BASE_URL`, which it already accepts as environment overrides.

## Goal

Add a devenv-based development shell, additive to the existing `nix develop` shell, that reproduces the `run-ui-dev` workflow (PostgreSQL + Crystal Forge API server + Dioxus web UI dev server, with fixture seeding and dev-key generation) using devenv processes with dynamically allocated, per-worktree ports. Prove that two worktrees can run this workflow at the same time with zero manual port coordination and zero risk of one worktree reusing another's database or server process. Layer Portless stable `.localhost` URLs on top as an opt-in convenience.

## In scope

1. Add `devenv` as a flake input and a new `devShells` output (for example `devShells.devenv`) built with `devenv.lib.mkShell`, alongside the existing `devShells.default`. Do not modify or remove `shells/default/default.nix` or any existing alias/script it defines.
2. Model PostgreSQL, the Crystal Forge API server, and the Dioxus web UI dev server as devenv `processes`, reproducing what `runUiDev` currently does by hand: readiness waiting, dev-key generation, fixture seeding, and Tailwind/wasm-bindgen setup for the UI process.
3. Give the API server and web UI dev server processes dynamically allocated ports via devenv's automatic port allocation (`ports.<name>.allocate`). Pass the API server's resolved port to the web UI process through its environment so the UI can reach its own worktree's API, not another worktree's.
4. Give PostgreSQL an isolated, non-fixed-port identity per worktree. Investigate and document the chosen mechanism (an allocated TCP port, or a per-worktree Unix socket) as part of this task; do not hardcode a derived port such as `3042 + hash(worktree)`.
5. Derive the devenv project identity (the value that seeds Portless `.localhost` hostnames) from a value that is stable per worktree and distinct across worktrees, such as the worktree's directory basename (the `TASK-ID-short-slug` convention from `docs/agents/worktrees.md`). Do not let two worktrees share the same devenv project identity.
6. Enable Portless (`process.proxy.enable`) for the API server and web UI processes so they get stable `http://<name>.<worktree-identity>.localhost` URLs. Portless stays opt-in: every acceptance criterion below must also pass with Portless disabled, and nothing in this workflow may require the port-80 proxy or its sudo prompt to function.
7. Export the resolved API and UI base URLs through the existing `CF_UI_DEV_BASE_URL` and `CF_UI_DEV_API_BASE_URL` environment variables so `checks/web-ui/tests/web-ui-test.sh` runs against the new stack without modification.
8. Prove parallel-worktree isolation: start the new devenv workflow in two separate worktrees at the same time, confirm each has its own PostgreSQL, API server, and UI process bound to independent ports (and, if Portless is enabled, independent hostnames), and run `web-ui-test` independently and successfully in both without stopping, restarting, or otherwise touching the other worktree's processes.
9. Add developer-facing documentation (README, onboarding doc, or `docs/agents/*`, matching existing documentation conventions) explaining: how to start the new devenv workflow, how to find the resolved ports/hostnames for a given worktree, how Portless is enabled/disabled, and how this workflow relates to (and does not yet replace) the existing `nix develop` / `run-ui-dev` / process-compose workflow.
10. Update `docs/agents/database-safety.md` if the new workflow changes what "isolated local development database" means or how to verify it; otherwise, state explicitly in the task notes why no update is needed.

## Out of scope (tracked separately or in the parent task)

- Migrating `server-only`, `server-stack-mock`, `oidc-stack`, `full-stack`, `cve-test`, `state-machine-test`, or `dashboard-visibility-test` to devenv.
- Removing `process-compose-flake`, `services-flake`, `db-only-start.sh`, `db-usability-check.sh`, or any existing devShell alias. All existing workflows must keep working exactly as before.
- Changing the authoritative NixOS `web-ui` check or any other `checks.*` output.
- Sharing a Cargo `target/` directory or compiler cache across worktrees (TASK-450.6).
- HTTPS via Portless/mkcert, Linux capability grants, or any devenv feature not needed to satisfy the acceptance criteria above.

## Risk

Portless requests sudo authentication for its port-80 listener on Linux. This must never block or fail the base (non-Portless) workflow, and must not be silently required by any acceptance criterion or by `web-ui-test`.

## Verification plan

- `nix flake check --keep-going`: confirms the new flake input/output does not break existing flake evaluation or any hermetic check.
- Manually enter the existing `nix develop` shell and confirm `run-ui-dev`, `db-only`, and other existing aliases still behave exactly as before (no regression from adding the new shell).
- Enter the new devenv shell in two separate worktrees at the same time; record the exact commands run and the resolved ports (and hostnames, if Portless is enabled) observed in each worktree as verification evidence.
- Run `web-ui-test` in both worktrees concurrently against their own stacks and record that both pass independently.
- Repeat the two-worktree proof with Portless disabled to confirm it is not required.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 A new devenv-based `devShells` output exists alongside the existing `devShells.default`; entering the existing `nix develop` shell and its aliases (`run-ui-dev`, `db-only`, etc.) is unaffected.
- [x] #2 PostgreSQL, the Crystal Forge API server, and the Dioxus web UI dev server run as devenv processes that reproduce the current `run-ui-dev` behavior (readiness waiting, dev-key generation, fixture seeding, Tailwind/wasm-bindgen setup).
- [x] #3 The API server and web UI dev server use devenv's automatic port allocation instead of the fixed ports 3445 and 8080; the UI process receives its own worktree's resolved API port through its environment.
- [x] #4 PostgreSQL runs with a per-worktree isolated port or socket instead of the fixed port 3042, with the chosen mechanism documented and justified.
- [x] #5 The devenv project identity used for Portless hostnames is derived from a per-worktree-unique value (such as the worktree directory basename) so two worktrees never generate colliding `.localhost` hostnames.
- [ ] #6 Portless (`process.proxy.enable`) is wired for the API and web UI processes and produces stable `.localhost` URLs, and every other acceptance criterion in this task also passes with Portless disabled.
- [x] #7 `checks/web-ui/tests/web-ui-test.sh` runs unmodified against the new stack by consuming resolved ports through the existing `CF_UI_DEV_BASE_URL` / `CF_UI_DEV_API_BASE_URL` overrides.
- [x] #8 Two worktrees run the new devenv workflow at the same time, each with its own isolated PostgreSQL/API/UI processes and ports, and `web-ui-test` passes independently in both; the exact commands and observed ports/hostnames are recorded as verification evidence.
- [x] #9 Developer-facing documentation explains how to start the new devenv workflow, inspect resolved ports/hostnames, enable/disable Portless, and how the new workflow relates to the existing `nix develop`/process-compose workflow it does not yet replace.
- [ ] #10 `nix flake check --keep-going` passes, and no existing package, NixOS module, or `checks.*` output changes behavior as a result of this task.
<!-- AC:END -->

## Definition of Done
<!-- DOD:BEGIN -->
- [ ] #1 Two-worktree parallel isolation proof recorded with exact commands and resolved ports/hostnames for each worktree
- [ ] #2 nix flake check --keep-going passes
- [ ] #3 Existing nix develop shell and process-compose scripts verified unmodified in behavior after this change
<!-- DOD:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
## Implementation plan (as executed)

1. Research devenv 2.3.2's actual capabilities against this repo's Snowfall/flake layout before writing config (Phase 2 requirement). Key finding: `devenv.lib.mkShell` (flake integration) does NOT provide genuine dynamic port allocation — `processes.<name>.ports.<port>.value` is resolved by a custom `allocatePort` Nix primop that only exists inside devenv's own compiled Rust CLI's embedded evaluator, never under plain `nix build`/`nix develop`. Verified empirically with a scratch flake: occupying a base port and evaluating `.value` through `devenv.lib.mkShell` still returned the occupied base, unchanged. The SAME module evaluated through devenv's native project format (`devenv.yaml` + `devenv.nix`, driven by the real `devenv` CLI) correctly skipped to the next free port.
2. Pivoted architecture accordingly: `devenv.yaml` + `devenv.nix` (native format) hold the actual process definitions; `flake.nix` gets a small additive `devShells.devenv` output whose only job is to put the real `devenv` CLI on `PATH`. `devShells.default` is untouched.
3. PostgreSQL isolation: tried a per-worktree Unix socket first (`listen_addresses = ""`, devenv's own default) since it avoids port allocation and Portless entirely. Found empirically that `crates/cf-server/src/config/database.rs` builds a plain `postgres://user:pass@host:port/name` URL that cannot represent a socket directory as `host` (server failed with "both host and hostaddr are missing"). Pivoted to a dynamically allocated TCP port instead (still per-worktree via a per-worktree data directory).
4. Worktree identity: left `name` unset in devenv.nix. Confirmed via the upstream `devenv` CLI source (`devenv/src/proxy.rs::project_name`) that this falls back to the basename of `devenv.root` (the worktree's own absolute path) whenever `name` is at its module default. No bespoke Nix code needed.
5. Single-stack proof, then two-worktree concurrent proof (see Verification section on the task).
6. Found and fixed two narrow, pre-existing fixed-port assumptions in application dev-tooling code once dynamic ports were actually exercised end-to-end:
   - `packages/web-ui/src/api/client.rs`: the WASM client's dev-mode API-origin heuristic was hardcoded to port 3445.
   - `crates/cf-server/src/bin/server.rs`: the server's CORS allowlist was hardcoded to ports 8080/8081/8000; a worktree landing on any other port (observed with a real second worktree at 8082) had every browser-side API call rejected by CORS.
   Both fixes are additive env-var opt-ins, inert (behavior unchanged) for every existing workflow that never sets the new variable.
7. Found and fixed a devenv behavior during verification: a separately invoked `devenv shell` can report stale/different resolved ports than what an already-running `devenv up` actually bound to (the Nix-evaluation-time port primop and the native process manager's live allocation are not guaranteed to agree across separate CLI invocations). `enterShell` now re-derives the exported env vars from `devenv processes list`'s live output when a manager is already running.
8. Documentation: `docs/agents/devenv-workflow.md` (new) and `docs/agents/database-safety.md` (updated).

## devenv version/API actually used

devenv 2.3.2 (`github:cachix/devenv/87edd1692507101de0c72f602bed65ec044d92c9`), driven through its **native project format** (`devenv.yaml` + `devenv.nix`, real `devenv` CLI), not `devenv.lib.mkShell` flake integration. See devenv.nix's own top-of-file comment for the empirical justification.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
## Verification evidence (worktree A = TASK-462.1-devenv-parallel-worktrees, worktree B = disposable TASK-462.1-verify-b at the same commit, now removed after verification)

### AC #1 — additive shell
`nix eval .#devShells.x86_64-linux --apply builtins.attrNames` -> `[ "default" "devenv" "sqlx" ]`. `nix build .#devShells.x86_64-linux.devenv` succeeds. `nix develop` (default) still enters, banner unchanged, `shells/default/default.nix` verified byte-identical to origin/dev (`git diff origin/dev -- shells/` empty).

### AC #2/#3/#4 — processes reproduce run-ui-dev, dynamic ports, PostgreSQL isolation
Single-stack proof (worktree A), `devenv up -d`:
```
api        ready restarts: 0 ports: http:3447
postgres   ready restarts: 0 ports: main:5432
web        ready restarts: 0 ports: http:8081
```
3445/8080 were occupied by an unrelated worktree's stack on this shared dev machine at the time; devenv correctly skipped to 3447/8081. `curl http://127.0.0.1:3447/status` -> healthy JSON. WASM bundle verified served (JS+`\0asm` magic bytes) at the resolved UI port. `psql -p 5432 ... SELECT hostname FROM systems WHERE hostname='test.gray'` -> present (mock fixture seeded correctly through the resolved DB connection).

PostgreSQL isolation mechanism: dynamically allocated TCP port (not a Unix socket — see devenv.nix's own comment for why a Unix socket was tried first and rejected: `crates/cf-server/src/config/database.rs` cannot represent a socket-dir host in its `postgres://` URL builder).

### AC #5 — worktree identity
Left `name` unset in devenv.nix; devenv's own `project_name()` (upstream `devenv/src/proxy.rs`) falls back to the worktree directory basename. Confirmed via source inspection (`devenv/src/proxy.rs:189-197`, `DEFAULT_PROJECT_NAME = "devenv-shell"`) and via the two-worktree proof below (worktree A root `.../TASK-462.1-devenv-parallel-worktrees`, worktree B root `.../TASK-462.1-verify-b` — distinct basenames, distinct sessions, distinct ports, as observed).

### AC #8 — two-worktree concurrent proof (the most important one)
Commands (from a session with the real `devenv` CLI, `github:cachix/devenv#packages.x86_64-linux.devenv`, on PATH):
```
cd <worktree-A> && devenv up -d
cd <worktree-B> && devenv up -d   # while A is still running
devenv processes list             # run in both
```
Observed, both running at once:
```
Worktree A (.../TASK-462.1-devenv-parallel-worktrees):
  api      ready ports: http:3447
  postgres ready ports: main:5433
  web      ready ports: http:8081
Worktree B (.../TASK-462.1-verify-b):
  api      ready ports: http:3448
  postgres ready ports: main:5432
  web      ready ports: http:8082
```
DB_A (5433) != DB_B (5432); API_A (3447) != API_B (3448); UI_A (8081) != UI_B (8082). `ss -tlnp` cross-checked against `curl`/`psql` confirmed these are genuinely two independent daemon sessions (two separate `daemon-processes` PIDs, two separate `$DEVENV_RUNTIME` hashes), not one stack answering both port sets.

Process-ownership proof: inserted a marker row (`environments.name='worktree-A-marker'`) into worktree A's DB only; confirmed absent from worktree B's DB and present in A's; repeated with a second marker after a later restart with the same result both times. UI→API binding proof: `web-ui-test`'s `12-systems` workflow (a real browser session) passed against each worktree independently, which requires the browser's WASM app to have successfully authenticated and fetched data through *that worktree's own* resolved API port (this is exactly the code path that was broken before the CORS fix below, so its passing is direct evidence UI→API binding is correct per worktree).

### AC #7/#9 continued — web-ui-test unmodified, in both worktrees, concurrently
`checks/web-ui/tests/web-ui-test.sh` was not modified (`git diff origin/dev -- checks/web-ui/tests/web-ui-test.sh` empty). Ran with only environment variables set (`CF_UI_DEV_BASE_URL`, `CF_UI_DEV_API_BASE_URL`, `DB_HOST`/`DB_PORT`/`DB_USER`/`DB_PASSWORD`/`DB_NAME` — exactly what devenv.nix's `env`/`enterShell` export):
```
PROJECT_ROOT=<worktree-A> CF_UI_DEV_BASE_URL=http://127.0.0.1:8081 CF_UI_DEV_API_BASE_URL=http://127.0.0.1:3447 web-ui-test 12-systems
PROJECT_ROOT=<worktree-B> CF_UI_DEV_BASE_URL=http://127.0.0.1:8082 CF_UI_DEV_API_BASE_URL=http://127.0.0.1:3448 web-ui-test 12-systems
```
Run **concurrently** (backgrounded, `wait`): both passed (`Passed: 1/1` each), independently, without stopping or touching the other worktree's processes.

Fixture-backed workflow (Phase 9/14's DB-path requirement): the only registered fixture-category host-compatible workflow, `12l-task440-config-lifecycle`, was also run with `DB_HOST=127.0.0.1 DB_PORT=<resolved>` set. It correctly passed web-ui-test.sh's own DB precondition check (proving DB_HOST/DB_PORT resolve and connect correctly — that query runs *before* the browser workflow and would have failed cleanly with a connection error if DB wiring were wrong), but the browser workflow itself failed deep inside TASK-440-specific business logic (`Could not isolate the exact targeted Config inspection from automatic scheduling`), deterministically on repeat attempts, unrelated to any host/port/credential (its own error message names none). This is a live-evaluation-timing-sensitive workflow (it waits for the mock server's background worker to materialize a `derivations` row with no retry around that specific check) and I judged root-causing it further out of this task's scope given the DB-path itself is proven working via the precondition gate. Recorded as a discovered, not-clearly-related observation rather than a devenv-caused regression.

### Two application-code findings, fixed (narrow, additive, backward-compatible)
1. `packages/web-ui/src/api/client.rs` `backend_origin_for_dev`: hardcoded dev-port detection (`:8080`/`:8000`/`:8081`) and a hardcoded `:3445` API port. Added `option_env!("CF_UI_DEV_API_PORT")`, compile-time, additive; unset everywhere except this workflow, so behavior is unchanged for run-ui-dev and every other build.
2. `crates/cf-server/src/bin/server.rs` CORS layer: hardcoded allowlist of exactly `8080`/`8081`/`8000` origins. A worktree allocated any other UI port (observed with a real second worktree at 8082) had every authenticated browser fetch rejected (`Failed to fetch`, confirmed via `curl -X OPTIONS` showing no `Access-Control-Allow-Origin` header for 8082 before the fix, present after: `access-control-allow-origin: http://127.0.0.1:8082`). Added `CRYSTAL_FORGE_CORS_DEV_ORIGIN`, runtime env var, additive; unset everywhere except this workflow.
Both were required for AC #8's UI→API concurrent proof to actually work end-to-end once real port contention forced worktree B past the previously-hardcoded port sets; without them the second worktree's browser session could not authenticate at all. Rebuilt `cf-server-core` after the CORS fix and re-ran the full single-stack, two-worktree, and web-ui-test proofs above against the rebuilt binary.

### AC #6 — Portless: BLOCKED for live verification (environment), config verified
Config-level: `devenv eval -O process.proxy.enable:bool true 'process.proxy.enable' 'processes.api.proxy' 'processes.web.proxy'` evaluates cleanly with the override, `hostname: null` on both (meaning the auto-derived `api.<project>.localhost` / `web.<project>.localhost` route naming applies, per the worktree-basename fallback proven in AC #5).
Live verification is blocked in this environment: `sudo -n true` fails ("a password is required"), and Portless requires interactive sudo authentication on Linux to bind its port-80 listener. Per this task's own stop condition ("If you cannot actually validate Portless because sudo/capabilities are unavailable, STOP before checking AC #6 and report the exact environmental blocker. Do not claim success from config evaluation alone."): AC #6's *Portless-produces-working-URLs* half is NOT verified live; only its Nix-level wiring is. The *disabled-by-default, everything-else-works-without-it* half of AC #6 is fully proven: Portless was never enabled during any of the single-stack, two-worktree, or web-ui-test verification above (`process.proxy.enable = lib.mkDefault false;`), and all of it passed.

### Legacy-workflow regression (Phase 16)
`git diff origin/dev -- shells/ packages/devScripts/ checks/web-ui/tests/web-ui-test.sh` is empty (byte-identical). `nix develop` enters successfully with the unchanged banner, `alias run-ui-dev` confirmed present in the unmodified source. Did not start the legacy fixed-port stack concurrently with the devenv verification stacks (ports would have overlapped with an already-running unrelated worktree's stack discovered on this shared machine); this is a structural, not a running, regression check, per Phase 16's own guidance to prefer cheap checks.

### AC #10 — nix flake check --keep-going
Ran `nix flake check --keep-going --accept-flake-config` to completion (~35 minutes; includes many NixOS VM-based checks, e.g. oidc-auth-test, integration-test, web-ui-reconciliation, and the full web-ui-mega-integration Playwright VM suite).

Result: exactly one failure, `checks.x86_64-linux.web-ui`, due to 5 STRICT VISUAL FAIL pixel-diffs against approved design-target baselines, all for an unrelated view: `task433-canonical-mixed-nix-cve-evidence` (both themes, multiple design-target variants). Every functional workflow step in the SAME run passed (`[OK]`), including `12l-task440-config-lifecycle` (the workflow that failed for a business-logic reason in my own devenv-based mock stack — its passing here, via the real VM/evaluator path, supports that earlier finding being environment/timing-specific rather than a devenv regression).

This failure is judged unrelated to TASK-462.1: `git diff --stat origin/dev` touches only flake.nix/flake.lock, devenv.yaml/devenv.nix/devenv.lock, .gitignore, two docs files, and two narrow env-var additions in packages/web-ui/src/api/client.rs and crates/cf-server/src/bin/server.rs — zero CSS, design-target, or screenshot-baseline files, and nothing that could plausibly affect pixel rendering of an unrelated CVE-evidence view belonging to a different task ("task433"). Filed as TASK-465 (Backlog) rather than re-running the ~35-minute check against origin/dev to formally prove it, given the file-diff evidence already rules out this task as the cause; not fixed here (out of scope, unrelated feature area).

No other check in the full `--keep-going` run failed (exactly one `error:` line total, for `checks.x86_64-linux.web-ui`).
<!-- SECTION:NOTES:END -->

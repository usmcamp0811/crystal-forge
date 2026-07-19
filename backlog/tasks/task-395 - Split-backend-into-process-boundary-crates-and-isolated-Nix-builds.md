---
id: TASK-395
title: Split backend into process-boundary crates and isolated Nix builds
status: In Progress
assignee: []
created_date: '2026-07-19 00:00'
updated_date: '2026-07-19 00:00'
lock: TASK-395-split-backend
labels:
  - backend
  - architecture
  - refactoring
  - build-performance
  - nix
  - cargo
  - sprint-ready
dependencies: []
references:
  - packages/default/Cargo.toml
  - packages/default/Cargo.lock
  - packages/default/default.nix
  - packages/default/src/config/
  - packages/default/src/models/builders.rs
  - packages/default/src/api/models.rs
  - packages/default/src/bin/agent.rs
  - packages/default/src/bin/builder.rs
  - packages/default/src/bin/server.rs
  - packages/default/src/bin/cf-keygen.rs
priority: high
ordinal: 395000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

`packages/default` is one Cargo package containing a large library plus the `server`, `builder`, `agent`, `test-agent`, and `cf-keygen` executables. Its manifest combines every component's dependencies, including Axum, SQLx/PostgreSQL, OIDC/JWT/Argon2, Reqwest, Tokio, WebSockets, crypto, Nix integration, and build/scanning machinery.

This monolithic crate prevents useful build isolation:

- Agent and builder changes invalidate and recompile the shared `crystal_forge` library, including unrelated server code.
- Agent and builder configuration loading is coupled to SQLx and PostgreSQL because `config` also creates and validates database pools and synchronizes configuration into server tables.
- Shared wire models are mixed with server persistence concerns; some nominal API DTOs derive `sqlx::FromRow`.
- The current Nix package builds all backend targets in one `buildRustPackage` derivation and enables server-oriented features for the common build. Building the agent or builder therefore also depends on unrelated backend sources and targets.
- A Cargo workspace split alone will not solve Nix invalidation while every executable shares one derivation and an unfiltered backend source.

The architecture no longer matches the operational reality: the server, remote builder, and deployment agent are independent distributed processes with different dependency and security boundaries.

This assessment was originally made against `dev` commit `bb58d92a` (2026-07-10). The current manifest still defines one `crystal-forge` package with all five binaries and the combined dependency set; implementation must revalidate all detailed module locations against its actual base commit.

## Goal

Split the backend along executable/process boundaries and make each production component independently selectable by Cargo and independently buildable by Nix. Targeted agent and builder checks/builds must stop compiling or being invalidated by unrelated server code and dependencies while preserving existing runtime behavior and deployed protocol compatibility.

The intended workspace shape is:

```text
packages/default/
├── Cargo.toml
├── Cargo.lock
└── crates/
    ├── cf-protocol/
    ├── cf-config/
    ├── cf-server/
    ├── cf-builder/
    ├── cf-agent/
    └── cf-keygen/
```

`test-agent` may remain a server/test-support target rather than becoming another production crate, provided it remains buildable by the appropriate test/check command.

## Recommended Implementation Order

1. Establish a virtual Cargo workspace while preserving current behavior.
2. Extract a deliberately small `cf-protocol` crate.
3. Extract a pure `cf-config` crate and move database operations into `cf-server`.
4. Extract `cf-agent`.
5. Extract `cf-builder`.
6. Move/rename the remaining backend package to `cf-server` and extract `cf-keygen`.
7. Split Nix component derivations and select Cargo packages explicitly.
8. Filter each Nix component's source to its transitive workspace source closure.
9. Record before/after Cargo timing and Nix invalidation evidence.

## Non-Goals

- Do not split every server domain module into its own crate.
- Do not introduce `cf-nix`, `cf-crypto`, or other speculative shared crates unless concrete code shared by multiple extracted components requires a real boundary; otherwise create a follow-up task.
- Do not migrate Nix builds to Crane as part of this task.
- Do not change API payloads, authentication, authorization, signing, builder sessions, job coordination, deployment behavior, or database schema.
- Do not restore direct database access to the remote builder.
- Do not optimize clean `cargo build --workspace` as the primary metric; targeted component work is the optimization target.
- Do not perform unrelated server module cleanup or refactoring.

## Architectural Constraints

- Dependency direction must place `cf-protocol` and `cf-config` below process crates. Neither foundational crate may depend on a process crate.
- `cf-protocol` must contain wire-level types only and must not depend on SQLx, PostgreSQL, Axum, Reqwest, or server modules.
- Do not move all of `api/models.rs` wholesale into `cf-protocol`. Separate serializable wire DTOs from server-only `sqlx::FromRow` database rows.
- `cf-config` may contain deserializable structures, defaults, environment/TOML loading, duration serialization, component sections, and validation that requires no external service.
- Database pool creation, database connectivity checks, and synchronization of systems/environments/flakes into PostgreSQL belong in `cf-server`.
- `cf-agent` must not depend on Axum, SQLx/PostgreSQL, OIDC, Argon2, JWT, server queries/background jobs, or CVE database handling.
- `cf-builder` remains API-only and DB-less. Existing builder session and server-issued job authorization boundaries must remain enforced.
- Preserve one workspace lock file and repository SQLx offline metadata for server queries.
- Preserve existing binary names and external Nix package/module interfaces unless an unavoidable compatibility issue is documented and approved before implementation.
- The server alone owns migrations, persistence, authorization policy, and server-side coordination.
- Source filtering must include workspace manifests/lock data plus the complete transitive local-crate closure for each component, and exclude unrelated process sources.

## Acceptance Criteria

- [ ] `packages/default/Cargo.toml` is a virtual workspace with one lock file and workspace members for `cf-protocol`, `cf-config`, `cf-server`, `cf-builder`, `cf-agent`, and `cf-keygen`.
- [ ] Existing production binaries remain available under their current names, and `test-agent` remains available through an explicitly documented package/target.
- [ ] Shared agent heartbeat/state, builder registration/session/job, derivation transport, status, and signing identifier wire types needed across processes live in `cf-protocol` without SQLx, Axum, Reqwest, PostgreSQL, or server dependencies.
- [ ] Database row types and SQLx derives remain server-only rather than leaking into `cf-protocol`.
- [ ] Pure configuration loading and service-independent validation live in `cf-config`.
- [ ] Database pool creation, database connection validation, and configuration-to-database synchronization have moved from shared configuration code into `cf-server`.
- [ ] Agent entrypoint, deployment behavior, heartbeat construction, signing, and agent-specific system inspection live in `cf-agent`.
- [ ] `cargo check -p cf-agent` succeeds and its resolved/compiled dependency graph excludes SQLx, PostgreSQL clients, Axum, OIDC, JWT, Argon2, server query/task modules, and server CVE database handling.
- [ ] Builder entrypoint, API client, metrics, build execution, derivation materialization/import, source worktree handling, and cache publication live in `cf-builder`.
- [ ] `cargo check -p cf-builder` succeeds and its resolved/compiled dependency graph excludes SQLx/PostgreSQL and server-only authentication, query, handler, and background-task code.
- [ ] Server handlers, queries, services, tasks, queueing, database models/migrations, flake management, vulnerability scanning, and hardening remain in `cf-server` unless a move is strictly required by the foundational crate boundaries.
- [ ] `cf-keygen` is independently selectable and does not pull in the server dependency graph.
- [ ] Server, builder, agent, keygen, and applicable test targets preserve their existing behavior and tests after import/path changes.
- [ ] Nix defines separate server, builder, and agent Rust derivations using explicit Cargo package selection; only the server build enables `embedded-ui`.
- [ ] `nix build .#agent` does not build the server or builder packages, and `nix build .#builder` does not build the server or agent packages.
- [ ] Agent and builder Nix derivations use filtered sources containing workspace metadata and only their transitive local-crate source closure; changing a server-only source file does not change/rebuild the agent or builder derivation.
- [ ] Existing flake package names, NixOS module wiring, wrappers, runtime paths, and service behavior continue to work.
- [ ] Before/after Cargo `--timings` reports are retained or summarized in the MR for clean targeted checks of agent, builder, and server, including command, base commit, machine context, elapsed time, and compiled crate count.
- [ ] The evidence demonstrates that targeted agent and builder checks no longer compile the server crate/dependency set; any timing regression or failure to improve targeted builds is explained before review.
- [ ] No database migration or public API compatibility break is introduced.
- [ ] Developer documentation names the new crate boundaries and gives the targeted Cargo and Nix commands for each process.

## Impact Areas

- Backend Cargo manifests, lock file, module paths, tests, and SQLx metadata under `packages/default`.
- Shared agent/server and builder/server protocol models.
- Configuration loading and server database bootstrap/synchronization.
- Agent, builder, server, test-agent, and key-generation entrypoints.
- `packages/default/default.nix`, top-level flake package exports, NixOS module package references, and relevant checks.
- Backend development and CI commands that currently select binaries from the monolithic package.

## Risk Level

High. This is a broad structural change across all backend executables and Nix packaging. Incorrect dependency movement could break wire compatibility, SQLx metadata, feature selection, embedded UI packaging, NixOS services, or security-sensitive agent/builder authorization and signing paths. Keep moves mechanical, preserve behavior, and verify each extraction incrementally.

## Dependencies

- No known backlog-task dependency.
- Before entering `To Do`, rebase the specification against the current `dev` layout and identify in-flight backend/Nix tasks likely to conflict.
- If the resulting implementation is too large for one reviewable MR, decompose this task into ordered child tasks that retain these boundaries and end-state acceptance criteria before implementation begins.

## Verification Plan

Capture a baseline before moving code, then repeat equivalent package-targeted checks after extraction. Run through the repository Nix development environment.

```bash
nix develop -c cargo clean --manifest-path packages/default/Cargo.toml
nix develop -c cargo check --manifest-path packages/default/Cargo.toml --bin agent --timings
nix develop -c cargo check --manifest-path packages/default/Cargo.toml --bin builder --timings
nix develop -c cargo check --manifest-path packages/default/Cargo.toml --bin server --timings
```

After the split:

```bash
nix develop -c cargo fmt --manifest-path packages/default/Cargo.toml --all -- --check
nix develop -c cargo check --manifest-path packages/default/Cargo.toml -p cf-agent --all-targets --timings
nix develop -c cargo check --manifest-path packages/default/Cargo.toml -p cf-builder --all-targets --timings
nix develop -c cargo check --manifest-path packages/default/Cargo.toml -p cf-server --all-targets --timings
nix develop -c cargo check --manifest-path packages/default/Cargo.toml -p cf-keygen --all-targets
```

- Run targeted tests for `cf-protocol`, `cf-config`, `cf-agent`, and `cf-builder` plus the full server backend suite with `SQLX_OFFLINE=true` where applicable.
- Inspect package/dependency metadata (for example with `cargo metadata` and package-specific dependency inspection) to prove forbidden server dependencies are absent from agent and builder graphs.
- Build the exported server, agent, and builder flake packages independently with `--no-link`.
- Perform a controlled Nix invalidation check: build agent and builder, modify only a disposable server-only source input in the task worktree, and show that their derivations remain unchanged; restore the disposable modification afterward.
- Run the relevant NixOS/module checks for all three services.
- Run `nix flake check --keep-going` because this task changes workspace packaging, flake outputs, NixOS package wiring, and cross-package interfaces.
- Smoke-test server startup, authenticated remote-builder registration/job polling, and agent heartbeat/deployment communication using the repository integration environment.

## Notes

The expected benefit is large for targeted `cargo check -p cf-agent`, `cargo check -p cf-builder`, server checks after agent/builder-only edits, and source-filtered component Nix builds. A clean full-workspace build and release-server link are not expected to improve materially.

Keep this task in `Backlog` until a human selects it for a sprint. Given its size and risk, decomposition into sequential child tasks is encouraged at selection time, but the final child must complete the separate, source-filtered Nix derivations; stopping after only a Cargo workspace split does not satisfy the goal.
<!-- SECTION:DESCRIPTION:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Created from the backend build-isolation proposal reviewed against `dev` commit `bb58d92a`. No implementation has started.
<!-- SECTION:NOTES:END -->

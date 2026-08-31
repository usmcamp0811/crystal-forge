---
id: doc-23
title: Build Invalidation Graph and CI Feedback Latency Analysis
type: specification
created_date: '2026-08-31 22:37'
tags:
  - build
  - ci
  - nix
  - performance
  - developer-experience
---
## Purpose

This document records the analysis behind the build and CI latency work. It
describes the current invalidation edges in the Crystal Forge Nix build graph,
the reason each edge causes unnecessary rebuilds, and the target architecture.

Agents that implement a subtask of the parent build-latency task MUST read this
document. The subtasks are intentionally small; this document holds the shared
rationale that does not belong in any single subtask.

## Problem statement

A small source change can invalidate large Rust builds. Several checks compile
substantially the same Rust dependency graph more than one time. The result is
long feedback latency for both humans and automated agents.

The cause is the shape of the dependency graph, not raw compute capacity. Adding
build machines makes unnecessary work finish sooner. It does not remove the
unnecessary work. Correct the graph first.

## Verified current state

The observations below were confirmed against the `dev` branch. Each item names
the exact location.

### The server derivation consumes the unfiltered workspace

`packages/default/default.nix:65`

```nix
serverSrc  = src; # server builds the full workspace
```

`cf-agent-drv`, `cf-builder-drv`, and `cf-keygen-drv` already use `mkWorkspaceSrc`
to restrict their source to a transitive local-crate closure. `cf-server-drv`
does not. A change under `crates/cf-builder` or `crates/cf-agent` therefore
changes the derivation input of `cf-server-drv`, even though the `cf-server`
manifest depends locally on `cf-config` and `cf-protocol` only.

`serverSrcHash` at `packages/default/default.nix:85` is derived from `serverSrc`.
The embedded `SRC_HASH` value therefore changes for unrelated component edits.

### The server derivation depends on the web UI derivation

`packages/default/default.nix:110` and `packages/default/default.nix:119`

```nix
"--features" "cf-server/embedded-ui"
CRYSTAL_FORGE_UI_DIST = "${pkgs.crystal-forge.web-ui}/public";
```

A Dioxus source change changes `pkgs.crystal-forge.web-ui`, which changes the
`cf-server-drv` input, which invalidates the backend Rust build and every check
that boots a server. This edge affects the `integration`, `oidc-auth`, and
`web-ui` checks together.

`crates/cf-server/Cargo.toml:8` declares `embedded-ui` as an optional feature.
The crate therefore already supports a build without the embedded UI.

### Internal consumers depend on aggregate packages

`packages/default/default.nix:301-305`

```nix
server = pkgs.symlinkJoin {
  paths = [ cf-server-drv cf-builder-drv cf-keygen-drv ];
};
```

The `crystal-forge` aggregate joins all four component derivations.

Internal consumers reference the aggregates instead of the exact derivation they
need:

- `modules/nixos/crystal-forge/default.nix:341,406,411,438` use
  `pkgs.crystal-forge.default.server`.
- `modules/nixos/crystal-forge/default.nix:443` uses
  `pkgs.crystal-forge.default.agent`.
- `checks/integration/default.nix:21,82` include `pkgs.crystal-forge.default`
  although `checks/integration/default.nix:126` sets `build.enable = false`.
- `checks/web-ui/default.nix:135,189` include `pkgs.crystal-forge.default`.
- `lib/default.nix:128,467` and `lib/server-test-node/default.nix:180` include
  `crystal-forge.default`.
- `checks/xccdf-schema/default.nix:5` uses `pkgs.crystal-forge.default.server`.

A machine or test VM that needs only the server therefore receives a closure
that also contains the builder, the agent, and the key generator. Any change to
any component invalidates that closure.

### Rust dependencies are rebuilt with application source

The server package, the web UI package, and `server-regressions` each use
`pkgs.rustPlatform.buildRustPackage` independently. `buildRustPackage` produces a
single derivation whose input includes application source. A one-line `.rs`
change therefore changes the derivation hash and discards the compiled Cargo
dependency tree from the previous build. There is no derivation whose input is
limited to `Cargo.lock` and the manifests.

`crane` is not present in `flake.lock` or in any Nix expression in the
repository.

### The devshell has no compiler cache

`shells/default` configures the Rust toolchain but sets neither `RUSTC_WRAPPER`
nor `CARGO_TARGET_DIR`. `sccache` is not referenced anywhere in the repository.

The agent workflow uses one dedicated worktree per task. At the time of writing
there are eight task worktrees plus `main` and `dev`. Each worktree keeps a
private `target/` directory, so each worktree recompiles the same dependency
crates independently.

### CI has no change-based gating and no cancellation

`.gitlab-ci.yml:182-188` runs a parallel matrix of `integration`, `oidc-auth`,
`server-regressions`, and `web-ui`.

`.gitlab-ci.yml:215-217` gates that job with `only: [merge_requests, main]`.

There is no `rules:changes` clause and no `interruptible: true` anywhere in
`.gitlab-ci.yml`. Every push to a merge request therefore starts the complete
heavy matrix, and a superseded pipeline keeps running after a newer commit
arrives.

The heavy checks are genuinely expensive:

- `checks/integration` boots a server VM with `virtualisation.memorySize = 8096`
  plus PostgreSQL, a Git server, and Grafana.
- `checks/oidc-auth` boots PostgreSQL, Keycloak, and Crystal Forge VMs.
- `checks/web-ui` uses `virtualisation.memorySize = 20480` and runs Playwright,
  Chromium, screenshot capture, and design-parity work.

### Reporting jobs sit in the fast feedback path

`packages/coverage/default.nix:38-47` runs `cargo tarpaulin --all-features
--workspace`. The wrapper catches Tarpaulin failure and the script ends with
`exit 0` at `packages/coverage/default.nix:262`.

`packages/code-metrics/default.nix:67` runs `cargo clippy --all-targets
--all-features` and appends `|| true`.

Both jobs are gated by `only: [merge_requests, main]`. Each therefore compiles a
large Rust graph on every push while returning no pass or fail signal that can
block a merge. They are reporting operations placed in a correctness-gate
position.

## Target architecture

```text
                         Cargo.lock and manifests
                                  |
                                  v
                        +--------------------+
                        | backend cargo deps |
                        +---------+----------+
                                  |
              +-------------------+-------------------+
              v                   v                   v
        cf-server-core        cf-builder           cf-agent
              |
              +---------------> unit and regression tests
              +---------------> integration check
              +---------------> oidc-auth check


               web-ui Cargo.lock
                      |
                      v
               web-ui cargo deps
                      |
                      v
                    web-ui
                      |
              +-------+--------+
              v                v
       ui-screenshots     web-ui-fast


                    web-ui
                      |
                      v
            cf-server-embedded-ui
                      |
                      v
              full review gate
```

Two principles control the target design:

1. Expensive dependency compilation MUST depend on dependency metadata only. It
   MUST NOT depend on arbitrary application source.
2. Unrelated components MUST NOT appear in the same package closure only for
   packaging convenience.

Aggregate packages MAY remain as public compatibility outputs. Internal services,
modules, and checks MUST NOT depend on them.

## Verification-level policy

The repository already tells agents to prefer targeted checks
(`docs/agents/verification.md`). The policy is currently prose. The intent is to
express it as named commands so it is harder to ignore.

| Level             | When                        | Latency target        |
| ----------------- | --------------------------- | --------------------- |
| `verify-fast`     | During coding               | Seconds to one minute |
| `verify-component`| After a logical unit        | A few minutes warm    |
| `verify-full`     | Once before Review          | Release-scale         |

`nix flake check` is a release-scale operation. It MUST NOT be used as an
iterative debugging command.

## Implementation order and rationale

The order matters. Later phases only pay off after earlier phases remove the
false invalidation edges.

| Phase | Work                                        | Priority |
| ----- | ------------------------------------------- | -------- |
| 1     | Filter `serverSrc`                          | P0       |
| 2     | Remove aggregate dependencies from internals| P0       |
| 3     | Split core server from embedded-UI server   | P0       |
| 4     | Crane dependency derivation for the backend | P0       |
| 5     | `server-regressions` reuses Crane artifacts | P1       |
| 6     | `sccache` in the devshell                   | P1       |
| 7     | CI change-based gating and cancellation     | P1       |
| 8     | Coverage and complexity off the fast path   | P1       |
| 9     | Attic wiring for project derivations        | P1       |
| 10    | Remote builders                             | P2       |

Phase 9 depends on phase 4. Before the Crane split, a one-character source change
changes the `cf-server` derivation hash, so the cache always misses for the
expensive dependency compilation. After the split, the dependency derivation hash
is stable across source-only changes and the cache substitutes it.

Phase 10 is deliberately last. Adding builders before the graph is corrected only
makes unnecessary work finish sooner.

## Constraints that MUST hold

- Flake output names that NixOS modules, systems, and test infrastructure consume
  MUST keep working. Removing an aggregate output is a breaking change.
- The production server MUST continue to serve the embedded web UI. At least one
  authoritative check MUST prove the combination of the production server binary,
  the embedded production WASM, and a real browser before merge.
- Reducing a test VM closure MUST NOT remove a binary that the VM actually
  executes. Verify what each VM runs before narrowing its closure.
- Coverage and complexity reporting MUST remain available. Moving a job out of
  the per-push path MUST NOT delete the report.
- `SRC_HASH` semantics change when `serverSrc` is filtered. Any consumer that
  compares a server source hash MUST be reviewed.

## References

- Crane API reference: https://crane.dev/API.html
- Cachix binary cache concepts: https://docs.cachix.org/what-is-a-binary-cache
- Nix GitLab CI caching guidance: https://nix-gitlab-ci.projects.tf/caching/

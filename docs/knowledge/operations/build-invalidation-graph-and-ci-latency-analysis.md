---
type: Design Specification
title: "Build invalidation graph and CI feedback latency analysis (doc-23)"
description: "Pointer to the retained Backlog document doc-23: the Nix build graph edges that cause needless Rust rebuilds, the target architecture, verification levels, implementation phases, and constraints for the build-latency work."
tags:
  - crystal-forge
  - build
  - ci
  - nix
  - performance
implementation_status: partial
generated:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-03T22:58:30-05:00
sources:
  - id: origin
    resource: "Crystal Forge repository file backlog/docs/build/build-invalidation-graph/doc-23%20-%20Build-Invalidation-Graph-and-CI-Feedback-Latency-Analysis.md at commit 3b23d36f"
    title: "Build Invalidation Graph and CI Feedback Latency Analysis"
---

# Build invalidation graph and CI feedback latency analysis (doc-23)

This concept is a navigation and status record. The retained Backlog document is the authoritative text. Backlog.md manages it by ID, so it stays at its original path:
[doc-23 - Build Invalidation Graph and CI Feedback Latency Analysis](<../../../backlog/docs/build/build-invalidation-graph/doc-23 - Build-Invalidation-Graph-and-CI-Feedback-Latency-Analysis.md>).

## What the document specifies

The document (created 2026-08-31) records the analysis behind the build and CI latency work. Agents that implement a subtask of the parent build-latency task must read it. Its sections are:

- **Purpose** and **Problem statement**: a small source change invalidates large Rust builds, and several checks compile the same Rust dependency graph more than once. The cause is graph shape, not compute capacity, so the graph is corrected before remote builders are added.
- **Verified current state** (observations confirmed against the `dev` branch at the time): the server derivation consumed the unfiltered workspace (`serverSrc = src`) and so changed `SRC_HASH` for unrelated edits; the server derivation depended on the web UI derivation through the `embedded-ui` feature; internal consumers (NixOS module, checks, test VMs) depended on aggregate packages; `buildRustPackage` rebuilt Cargo dependencies with application source and `crane` was absent; the devshell had no compiler cache (`sccache`); CI had no change-based gating (`rules:changes`) or cancellation (`interruptible`); and the coverage and code-metrics reporting jobs sat in the fast feedback path.
- **Target architecture**: an ASCII dependency diagram with a Crane-style dependency-only derivation, `cf-server-core`, `cf-builder`, `cf-agent`, a separate web UI dependency derivation, `ui-screenshots`, `web-ui-fast`, `cf-server-embedded-ui`, and a full review gate. Two principles control it: dependency compilation depends on dependency metadata only, and unrelated components do not share a package closure for convenience. Aggregate packages may remain as public compatibility outputs but internals must not depend on them.
- **Verification-level policy**: `verify-fast`, `verify-component`, and `verify-full` with latency targets. `nix flake check` is release-scale and not an iterative command.
- **Implementation order and rationale**: ten phases (P0: filter `serverSrc`, remove aggregate dependencies, split core and embedded-UI server, Crane dependency derivation; P1: `server-regressions` reuses Crane artifacts, `sccache`, CI gating and cancellation, coverage and complexity off the fast path, Attic wiring for project derivations; P2: remote builders). Phase 9 depends on phase 4, and phase 10 is deliberately last.
- **Constraints that MUST hold**: keep consumed flake output names, keep one authoritative check for server plus embedded UI plus a real browser, do not remove a binary a VM executes, keep coverage and complexity reporting available, and review consumers of `SRC_HASH`.
- **References**: Crane API, Cachix binary cache concepts, and Nix GitLab CI caching guidance.

## Implementation status

Status: **partial**. Evidence checked on the migration branch:

- Phase 1 exists. `serverSrc` is built with `mkWorkspaceSrc` in `packages/default/default.nix`.
- Phases 3 and 4 exist. `packages/default/default.nix` defines `cf-server-core-drv` (no embedded UI) and an embedded-UI variant, uses `craneLib.mkDummySrc` and `craneLib.buildDepsOnly`, and `flake.nix` declares the `crane` input. The document's statement that `crane` is absent from the repository is stale.
- Phase 2 is partly done. `lib/default.nix` and `lib/server-test-node/default.nix` use `cf-server-core-drv`. The NixOS module defaults to `cf-server-drv`, `cf-builder-drv`, and `cf-keygen-drv` rather than the aggregates. A complete audit of the listed consumers was not done.
- Phase 6 was not found. `sccache`, `RUSTC_WRAPPER`, and `CARGO_TARGET_DIR` do not appear under `shells/`.
- Phase 7 was not found. `.gitlab-ci.yml` contains no `rules:changes` or `interruptible` setting.
- Phase 8 was not found. `packages/code-metrics/default.nix` still appends `|| true` to its clippy run.
- Phases 5, 9, and 10 were not verified. `checks/server-regressions/default.nix` contains no Crane reference.
- The `verify-fast`, `verify-component`, and `verify-full` commands were not found in the repository.

The retained document still describes these as current problems. The verification pass must reconcile its "Verified current state" section with the code above.

## Related concepts

* [Remote builder execution strategies](../builders/remote-build-execution-strategies.md) - Phase 10 (remote builders) depends on the graph work this document describes.
* [S3 (MinIO) cache quickstart](../caches/s3-minio-cache-quickstart.md) - Binary cache usage that phase 9 (Attic wiring) extends.

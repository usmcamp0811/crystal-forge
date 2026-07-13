---
id: TASK-387
title: Remote builders consume server-bundled source archives instead of Git remotes
status: Backlog
assignee: []
created_date: '2026-07-09 00:00'
updated_date: '2026-07-09 00:00'
labels:
  - builders
  - remote-builders
  - source-delivery
  - stability
  - backend
  - architecture
dependencies:
  - TASK-384
references:
  - packages/default/src/handlers/api/builders.rs
  - packages/default/src/bin/builder.rs
  - packages/default/src/builder/api_client.rs
  - packages/default/src/models/builders.rs
  - packages/default/src/derivations/build.rs
  - packages/default/src/derivations/cache.rs
priority: high
ordinal: 330000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

Remote API builders currently re-fetch source directly from the original Git remote for source-based build strategies. The server sends source identity metadata (`repo_url`, `commit_hash`, flake target, mirror id), but `archive_url` and `archive_sha256` are always `None`, so `ServerBundledArchive` is effectively a non-functional stub.

This violates the intended Crystal Forge boundary: remote builders should only talk to the Crystal Forge server, not GitLab/GitHub/origin remotes. It also makes remote builders less stable because they depend on direct Git remote availability, credentials, fetch consistency, local mirror state, and network path reliability.

Observed failure pattern motivating this task:

- Repeated builder failures for the same derivation, e.g. `nixos-system-daly-26.05.20260709.1d1f350.drv`, with repeated job attempts failing after spawning `nix build`.
- Builds falling back from server cache publish to archive download after upstream/server timeouts such as HTTP 524.
- WebSocket log streaming also falling back to HTTP, making diagnosis harder.

The source delivery path is not deterministic or isolated enough for stable remote builders.

## Goal

Implement production-ready server-bundled source delivery for remote builders. Source-based remote builds should fetch immutable source archives only from Crystal Forge, verify their hash, unpack locally, and evaluate/build from that local source.

Normal remote builder operation must not require direct access to Git remotes or repository credentials.

## Non-Goals

- Do not redesign the build scheduler.
- Do not remove existing source strategies until the new strategy is proven and migration-safe.
- Do not change deployment agent behavior.
- Do not require builders to access the database.
- Do not weaken builder request authentication or source verification.
- Do not silently fall back to direct Git remote access unless an explicit compatibility flag enables it.

## Acceptance Criteria

- [ ] Server creates or reuses an immutable source archive for each `(flake_id/repo_url, commit_hash)` needed by remote builders.
- [ ] Server exposes an authenticated builder-download endpoint for source archives.
- [ ] `verified_source_identity_for_derivation()` populates `archive_url` and `archive_sha256` for `ServerBundledArchive`.
- [ ] Remote builder `ServerBundledArchive` strategy downloads the archive from Crystal Forge, verifies `archive_sha256`, unpacks into its source worktree/cache area, and evaluates the requested flake target from the unpacked source.
- [ ] In the intended production strategy, remote builders do not run `git clone`, `git fetch`, or otherwise contact original Git remotes.
- [ ] If archive creation, download, unpack, or hash verification fails, the build fails with a clear phase-specific error message.
- [ ] Existing `SourceReEvaluateVerified` and `ServerDerivation` behavior remains backwards-compatible while the new path is introduced.
- [ ] Tests cover archive URL population, authenticated download, hash mismatch rejection, builder unpack/eval behavior, and no-direct-git behavior for the server-bundled strategy.
- [ ] Operational docs/config explain how to enable the server-bundled strategy for remote builders and how archive retention/cleanup works.

## Architectural Constraints

- Builder remains DB-less and communicates only through authenticated Crystal Forge server APIs.
- Archive identity must be deterministic and tied to repo URL + exact commit hash.
- Archive hash verification is mandatory before evaluation/build.
- Server-side source archive generation must use repository credentials already managed by Crystal Forge; builders must not need those credentials.
- Source archive storage, retention, cleanup, and cache invalidation must be explicit and bounded.
- Preserve separation between builder API models, source archive service/storage, and builder execution strategy code.
- Avoid hidden global state; source archive storage should be configured through existing server configuration patterns.

## Impact Areas

- Server builder API: `packages/default/src/handlers/api/builders.rs`.
- Builder runtime/source strategy: `packages/default/src/bin/builder.rs`.
- Builder/source API models: `packages/default/src/models/builders.rs`.
- Source archive storage/cache service (new module likely required).
- Authenticated download routing.
- Remote builder strategy tests and integration checks.

## Risk Level

High. This changes remote builder source acquisition and affects build determinism, network boundaries, credentials, and cache behavior. Roll out behind an explicit strategy/config flag and keep existing strategies available during migration.

## Dependencies

- TASK-384 should land first so deployment progress/cache-push lifecycle fixes are in place.
- Requires server-side access to flake source credentials for private repositories.

## Verification Plan

- Unit tests for source archive identity/hash helpers.
- API tests for authenticated archive download and unauthorized rejection.
- Builder strategy tests for hash verification, unpack path handling, and failure on missing/mismatched archive.
- Integration test with a remote/API builder configured for `ServerBundledArchive`, proving it builds without direct Git remote fetches.
- `cargo fmt --all -- --check`.
- Targeted backend tests for source archive API and builder strategy.
- `SQLX_OFFLINE=true cargo check --all-targets`.
- `nix build .#packages.x86_64-linux.server --no-link`.
- `nix flake check --keep-going` if Nix packaging, modules, or integration checks are touched.

## Notes

Concrete trace from investigation:

- Server sends `repo_url`, `commit_hash`, flake target, and mirror id.
- Server does not currently send source content.
- `archive_url` and `archive_sha256` are always `None`.
- Builder `SourceReEvaluateVerified` path creates/updates a bare mirror by running `git clone --bare` / `git fetch` against the original Git remote, then creates a detached worktree and runs `nix eval` locally.
- Current `ServerBundledArchive` intent is visible in tests, but production code does not populate the archive fields.

This task makes remote builders more stable and enforces the intended architecture: remote builders talk to Crystal Forge, not directly to Git remotes.
<!-- SECTION:DESCRIPTION:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Created as a high-priority Sprint-Ready follow-up from TASK-384 runtime investigation. Keep in Backlog until a human selects it into To Do after TASK-384.
<!-- SECTION:NOTES:END -->

---
id: TASK-374
title: 'HOTFIX: Fix builder API-mode NixOS module/runtime contract'
status: Review
assignee:
  - gpt-5.5
created_date: '2026-06-27 16:04'
updated_date: '2026-06-27 21:51'
labels:
  - bug
  - hotfix
  - nixos-module
  - builder
  - api-mode
milestone: Hotfix
dependencies: []
references:
  - /home/mcamp/code/campground/systems/x86_64-linux/webb/default.nix
  - >-
    /home/mcamp/code/campground/fmf-flake/modules/nixos/services/crystal-forge/default.nix
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/288'
modified_files:
  - modules/nixos/crystal-forge/default.nix
  - packages/default/src/config/builder.rs
  - packages/default/src/builder/api_client.rs
  - packages/default/src/handlers/api/builders.rs
  - packages/default/src/queries/builders.rs
  - packages/default/src/bin/server.rs
  - packages/default/src/bin/builder.rs
  - packages/default/src/models/builders.rs
priority: high
ordinal: 100
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem Statement

Builder-only deployments configured with `fmf.services.crystal-forge.build.api_mode = true` still start the Crystal Forge builder in legacy direct-database mode or fail parsing configuration.

Observed on webb:

- Builder logs show `Starting builder in deprecated legacy direct-database mode` despite `build.api_mode = true`.
- Runtime fails with `both host and hostaddr are missing` after falling back to database mode.
- Earlier config generation also caused `missing configuration field "client.private_key"` when the common suite enabled the client without a generated key.
- Builder pre-start reports `runuser: may not be used by non-root users`, indicating the pre-start Attic test still calls `runuser` even though the service may already run as the service user.

## Goal

Fix Crystal Forge so a NixOS builder configured for API mode emits the runtime configuration the builder binary actually consumes and does not require direct database access.

## Explicit Non-Goals

- Do not change campground host configuration except as needed later to consume the fixed Crystal Forge behavior.
- Do not open PostgreSQL access from builder hosts as a workaround.
- Do not redesign builder registration UI or database schema unless the existing runtime requires a minimal compatible change.
- Do not refactor unrelated NixOS module sections.

## Architectural Constraints

- Keep builder API mode free of direct database dependencies.
- Preserve backward compatibility for explicit legacy database mode (`api_mode = false`).
- Keep NixOS module changes scoped and deterministic.
- Do not introduce hidden global state or unpinned dependencies.

## Impact Areas

- `modules/nixos/crystal-forge/default.nix`
- Builder runtime config shape if the NixOS module cannot satisfy current expectations cleanly
- NixOS integration checks for builder API mode

## Risk Level

High: deployment blocker for additional builders, affects NixOS module generation and runtime startup behavior.

## Dependencies

None known.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Existing combined server/local-builder deployments that do not set `build.api_mode` remain in legacy database mode on upgrade; distributed builders can explicitly set `build.api_mode = true`.
- [x] #2 Generated Crystal Forge config includes the builder API-mode fields consumed by the builder runtime when API mode is explicitly enabled: enabled API mode, private key path, and server URL.
- [x] #3 A deployed API-mode builder can start without a local `builder_id`, derive its public key, and resolve its server-side builder ID after the operator registers that public key in the UI/API.
- [x] #4 In API mode, builder startup does not require or attempt direct PostgreSQL/database configuration.
- [x] #5 Builder API key generation remains non-interactive and writes files with restrictive permissions.
- [x] #6 Builder pre-start does not call `runuser` from a non-root service context.
- [x] #7 Targeted Rust and NixOS evaluation tests verify the generated config/API-mode path and upgrade-compatible legacy default.
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Addressed re-review test-quality finding in commit 2c15a362 (`test: verify builder bootstrap signing interoperability`) and pushed to MR !288.

Changes:
- Made `BuilderApiClient::sign_bootstrap_request` visible within the crate.
- Added `resolve_builder_request_accepts_client_generated_bootstrap_signature`, which signs with the client helper, builds the same headers sent by the client, and verifies through the server handler verifier.
- Updated MR description and verified via `glab mr view 288` that the description now reflects the compatibility default and cross-module interoperability test.

Verification:
- `SQLX_OFFLINE=true nix develop -c cargo test handlers::api::builders::tests::resolve_builder_request_accepts_client_generated_bootstrap_signature --lib` passed: 1 passed, 0 failed.
- `SQLX_OFFLINE=true nix develop -c cargo test handlers::api::builders::tests --lib` passed: 13 passed, 0 failed.
- `SQLX_OFFLINE=true nix develop -c cargo test builder::api_client::tests --lib` passed: 6 passed, 0 failed.
- `git diff --check` passed.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Implemented API-mode builder runtime config and public-key builder ID resolution while preserving upgrade compatibility. After review, restored `build.api_mode = false` as the default so existing combined server/local-builder deployments do not silently require public-key registration; distributed builders can explicitly set `api_mode = true`. Added focused bootstrap authentication tests, including a cross-module client/server signing interoperability test, and NixOS eval regression coverage. MR: https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/288
<!-- SECTION:FINAL_SUMMARY:END -->

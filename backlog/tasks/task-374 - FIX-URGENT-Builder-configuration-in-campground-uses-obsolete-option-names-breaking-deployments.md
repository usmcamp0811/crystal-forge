---
id: TASK-374
title: 'HOTFIX: Fix builder API-mode NixOS module/runtime contract'
status: In Progress
assignee:
  - gpt-5.5
created_date: '2026-06-27 16:04'
updated_date: '2026-06-27 20:17'
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
modified_files:
  - modules/nixos/crystal-forge/default.nix
  - packages/default/src/config/builder.rs
  - packages/default/src/builder/api_client.rs
  - packages/default/src/handlers/api/builders.rs
  - packages/default/src/handlers/builder_request.rs
  - packages/default/src/queries/builders.rs
  - packages/default/src/bin/server.rs
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
- [ ] #1 With `build.enable = true`, the NixOS module configures builder API mode by default without requiring users to set `api_mode = true`.
- [ ] #2 Generated Crystal Forge config includes the builder API-mode fields consumed by the builder runtime: enabled API mode, private key path, and server URL.
- [ ] #3 A deployed builder can start without a local `builder_id`, derive its public key, and resolve its server-side builder ID after the operator registers that public key in the UI/API.
- [ ] #4 In API mode, builder startup does not require or attempt direct PostgreSQL/database configuration.
- [ ] #5 Builder API key generation remains non-interactive and writes files with restrictive permissions.
- [ ] #6 Builder pre-start does not call `runuser` from a non-root service context.
- [ ] #7 A targeted Rust and/or NixOS evaluation test verifies the generated config/API-mode path.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Make the NixOS builder deployment path API-first: `build.enable = true` should generate a builder API key and API-mode runtime config without requiring users to set `api_mode = true`.
2. Keep the operator-approved public-key joining flow: the deployed builder generates/keeps a local private key, prints the derived public key, and the operator creates/approves the builder in Crystal Forge UI using that public key.
3. Add server-side lookup by builder public key so a running builder can discover its server-assigned UUID after the operator registers the public key.
4. Add a minimal builder-auth bootstrap endpoint (for example `POST /api/v1/builders/resolve-id`) that accepts the derived public key plus a signed/timestamped request, verifies the signature against that public key, looks up the registered builder, and returns the builder UUID only if the builder is enabled/allowed.
5. Update `BuilderApiClient` and `BuilderConfig` so `builder_id` is optional locally. At startup, load the private key, derive the public key, resolve the server-side builder ID, then use the existing ID-based signed endpoints for heartbeat/jobs/logs.
6. Update the NixOS module to generate TOML `[builder]` API-mode config consumed by the runtime: `enable_api_mode = true`, `private_key_path`, and `server_url`, without requiring local `builder_id` or database configuration.
7. Fix the builder pre-start Attic check so it does not call `runuser` from the non-root service context; run `attic login list` directly with the service environment.
8. Add targeted tests for public-key builder lookup/bootstrap auth where practical, and run targeted Rust/Nix verification for the touched paths.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Scope clarification from human: API mode is the only supported/intended builder deployment mode. NixOS users should not need to set `api_mode = true`; enabling the builder should configure API mode by default.
<!-- SECTION:NOTES:END -->

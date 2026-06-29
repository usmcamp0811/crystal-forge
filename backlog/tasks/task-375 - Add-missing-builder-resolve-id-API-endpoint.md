---
id: TASK-375
title: Add missing builder resolve-id API endpoint and UI key persistence
status: In Progress
assignee: []
created_date: '2026-06-28 02:11'
updated_date: '2026-06-29 03:13'
labels:
  - bug
  - builder
  - api
  - ui
milestone: Builder API hotfix
dependencies: []
references:
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/289'
modified_files:
  - packages/default/src/handlers/api/builders.rs
  - packages/default/src/queries/builders.rs
  - packages/default/src/bin/server.rs
  - packages/web-ui/src/components/builders/edit_builder_modal.rs
  - packages/default/src/queries/systems.rs
  - packages/default/src/handlers/agent/heartbeat.rs
  - packages/default/src/config/deployment.rs
  - packages/default/src/deployment/agent.rs
  - modules/nixos/crystal-forge/default.nix
  - packages/default/src/queries/deployment.rs
  - packages/default/migrations/0144_add_desired_target_set_at.sql
priority: high
ordinal: 5500
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Problem: the API-mode builder client resolves its builder ID by calling POST /api/v1/builders/resolve-id with a signed public-key request, but the server did not expose a matching route/handler, causing deployed builders to fail with 405 Method Not Allowed. After that endpoint is available, the rollout still depends on the Builders UI persisting the generated builder public key; the UI save flow currently appears not to update the key.

Desired Outcome: the server exposes the resolve-id endpoint used by the API-mode builder client, validates the signed request against the registered builder public key, returns the matching builder ID, and the Builders UI can persist the generated public key so builders can start without any direct database fallback.

Non-Goals:
- Do not reintroduce legacy direct database fallback in the builder.
- Do not change unrelated builder job APIs.
- Do not modify campground/fmf deployment wiring.

Architectural Constraints:
- Keep builder DB access server-side only.
- Follow existing Axum handler and builder auth patterns.
- Reuse existing DTOs where possible.
- Keep UI changes scoped to public-key persistence and error reporting.

Impact Areas:
- packages/default/src/handlers/api/builders.rs
- packages/default/src/queries/builders.rs
- packages/default/src/bin/server.rs
- packages/web-ui/src/components/builders/edit_builder_modal.rs

Risk Level: medium

Verification Plan:
- SQLX_OFFLINE=true nix develop -c cargo check --bin server --bin builder
- SQLX_OFFLINE=true nix develop -c cargo test builder::api_client::tests --lib
- nix develop -c cargo check --target wasm32-unknown-unknown from packages/web-ui
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 POST /api/v1/builders/resolve-id exists on the server and no longer returns 405 for a valid signed builder request.
- [ ] #2 The endpoint resolves a registered builder by public key and returns its builder ID using the existing ResolveBuilderIdResponse DTO.
- [ ] #3 The endpoint rejects unknown, disabled, or invalidly signed builder requests without exposing direct database access to builder processes.
- [ ] #4 The builder remains API-only and does not reintroduce legacy direct-database fallback.
- [ ] #5 Changing a builder public key in the Builders UI and clicking Save persists the new key or surfaces a clear backend error.
- [ ] #6 Remote builder executes real builds via API with no DB pool: it fetches derivation payload, streams logs, reports progress, honors cancellation, and reports completion/failure entirely over HTTP/WebSocket.
- [ ] #7 Server exposes the endpoints required for remote builds (derivation payload, build progress heartbeat) and performs derivation completion/failure and cache-push queueing server-side.
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Addressed re-review findings and pushed commit b6ef5a3f (`fix: track desired target freshness separately`) to MR !289.

Changes:
- Added `systems.desired_target_set_at` migration.
- Manual desired-target freshness now uses `desired_target_set_at`, not generic `systems.updated_at`.
- Existing manual desired targets are migrated with no freshness timestamp so they are suppressed until explicitly set again.
- Desired-target writers set/clear `desired_target_set_at` with the target value.
- Stale manual target clearing is guarded by hostname, target value, and target timestamp to avoid deleting a concurrently replaced request.
- Renamed delay config/option to `post_agent_start_deployment_delay` to match process-start semantics.

Verification before push:
- `nix develop .#sqlx -c cargo sqlx prepare` passed against the local process-compose DB target; no tracked SQLx metadata changes were produced.
- `nix develop .#sqlx -c cargo test agent_desired_target_policy --lib` passed (4 focused tests).
- `nix develop .#sqlx -c cargo check --bin agent --bin server` passed with existing warnings.
- Nix eval for `services.crystal-forge.deployment.post_agent_start_deployment_delay = 90` returned `90`.
- `git diff --check --cached` passed.

Per user request, broader CI checks are left to run on the pushed MR branch.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: gpt-5.5
created: 2026-06-28 02:50
---
User requested folding the related Builders UI public-key persistence issue into TASK-375 because the API-mode rollout depends on registering the generated key from the UI.
---
<!-- COMMENTS:END -->

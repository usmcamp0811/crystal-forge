---
id: TASK-375
title: Add missing builder resolve-id API endpoint and UI key persistence
status: In Progress
assignee: []
created_date: '2026-06-28 02:11'
updated_date: '2026-06-28 04:52'
labels:
  - bug
  - builder
  - api
  - ui
milestone: Builder API hotfix
dependencies: []
modified_files:
  - packages/default/src/handlers/api/builders.rs
  - packages/default/src/queries/builders.rs
  - packages/default/src/bin/server.rs
  - packages/web-ui/src/components/builders/edit_builder_modal.rs
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

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Transport principle: WebSocket for responsiveness (live progress, instant cancel), HTTP for atomic source-of-truth + fallback.

1. Add BuildReporter trait abstracting in-build ops: report_progress(elapsed,target,last_activity) and is_cancelled(). Implement:
   - PgPoolReporter (server/local worker): existing update_build_heartbeat + get_build_job_status.
   - ApiBuildReporter (remote builder): WS-primary progress send + WS-received CancelRequested, HTTP fallback (append-logs progress / GET job-status poll) when WS down.
2. Refactor Derivation::build_with_log_sink / run_streaming_build / build_with_direct_nix_store to take &dyn BuildReporter instead of &PgPool. Keep server worker.rs working via PgPoolReporter.
3. Enrich next-job HTTP response to include full derivation build payload (derivation_path, type, name, id, store_path, etc.) so remote builder needs no DB read. Claim stays atomic HTTP.
4. Extend BuildStreamMessage with builder->server Progress and server->builder CancelRequested; update the per-job log WS handler to persist progress (update_build_heartbeat) and to push CancelRequested when a job is cancelling.
5. Server complete_job/fail_job/finalize handlers perform derivation completion/failure + cache-push queueing server-side (so builder never writes DB).
6. Move cache-push loop + CVE scanning server-side; remove from remote builder.
7. Rewrite bin/builder.rs to be DB-free: no db_pool(), no DB fallbacks; build via API payload + reporter; report results via HTTP.
8. Keep heartbeat/liveness (TASK-282) and atomic /next-job semantics intact.
9. Verify: SQLX_OFFLINE cargo check server+builder, targeted tests, web-ui check, then deploy to webb/reckless and confirm a real remote build runs with no DB connection.
<!-- SECTION:PLAN:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: gpt-5.5
created: 2026-06-28 02:50
---
User requested folding the related Builders UI public-key persistence issue into TASK-375 because the API-mode rollout depends on registering the generated key from the UI.
---
<!-- COMMENTS:END -->

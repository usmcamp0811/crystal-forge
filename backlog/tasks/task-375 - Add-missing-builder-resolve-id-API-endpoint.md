---
id: TASK-375
title: Add missing builder resolve-id API endpoint and UI key persistence
status: In Progress
assignee: []
created_date: '2026-06-28 02:11'
updated_date: '2026-06-28 02:50'
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
- Builders UI edit/save flow
- Frontend API client for builder updates if needed

Risk Level: medium

Verification Plan:
- SQLX_OFFLINE=true nix develop -c cargo check --bin server --bin builder
- SQLX_OFFLINE=true nix develop -c cargo test builder::api_client::tests --lib
- Targeted UI/client check for builder public key update save behavior where feasible.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 POST /api/v1/builders/resolve-id exists on the server and no longer returns 405 for a valid signed builder request.
- [ ] #2 The endpoint resolves a registered builder by public key and returns its builder ID using the existing ResolveBuilderIdResponse DTO.
- [ ] #3 The endpoint rejects unknown, disabled, or invalidly signed builder requests without exposing direct database access to builder processes.
- [ ] #4 The builder remains API-only and does not reintroduce legacy direct-database fallback.
- [ ] #5 Changing a builder public key in the Builders UI and clicking Save persists the new key or surfaces a clear backend error.
<!-- AC:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: gpt-5.5
created: 2026-06-28 02:50
---
User requested folding the related Builders UI public-key persistence issue into TASK-375 because the API-mode rollout depends on registering the generated key from the UI.
---
<!-- COMMENTS:END -->

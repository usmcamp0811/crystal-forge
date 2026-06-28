---
id: TASK-375
title: Add missing builder resolve-id API endpoint
status: To Do
assignee: []
created_date: '2026-06-28 02:11'
updated_date: '2026-06-28 02:14'
labels:
  - bug
  - builder
  - api
milestone: Builder API hotfix
dependencies: []
modified_files:
  - packages/default/src/handlers/api/builders.rs
  - packages/default/src/builder/api_client.rs
  - packages/default/src/models/builders.rs
priority: high
ordinal: 5500
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Problem: the API-mode builder client resolves its builder ID by calling POST /api/v1/builders/resolve-id with a signed public-key request, but the server does not currently expose a matching route/handler, causing deployed builders to fail with 405 Method Not Allowed.

Desired Outcome: the server exposes the resolve-id endpoint used by the API-mode builder client, validates the signed request against the registered builder public key, and returns the matching builder ID so builders can start without any direct database fallback.

Non-Goals:
- Do not reintroduce legacy direct database fallback in the builder.
- Do not change unrelated builder job APIs.
- Do not modify campground/fmf deployment wiring.

Architectural Constraints:
- Keep builder DB access server-side only.
- Follow existing Axum handler and builder auth patterns.
- Reuse existing DTOs where possible.

Impact Areas:
- packages/default/src/handlers/api/builders.rs
- API router wiring for builders endpoints
- packages/default/src/queries/builders.rs if no suitable lookup-by-public-key helper exists

Risk Level: medium

Verification Plan:
- SQLX_OFFLINE=true nix develop -c cargo check --bin server --bin builder
- SQLX_OFFLINE=true nix develop -c cargo test builder::api_client::tests --lib
- Targeted server/handler test for resolve-id if existing test utilities support it.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 POST /api/v1/builders/resolve-id exists on the server and no longer returns 405 for a valid signed builder request.
- [ ] #2 The endpoint resolves a registered builder by public key and returns its builder ID using the existing ResolveBuilderIdResponse DTO.
- [ ] #3 The endpoint rejects unknown, disabled, or invalidly signed builder requests without exposing direct database access to builder processes.
- [ ] #4 The builder remains API-only and does not reintroduce legacy direct-database fallback.
<!-- AC:END -->

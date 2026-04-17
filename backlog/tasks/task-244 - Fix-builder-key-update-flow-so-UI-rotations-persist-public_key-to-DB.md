---
id: TASK-244
title: Fix builder key update flow so UI rotations persist public_key to DB
status: Backlog
assignee: []
created_date: '2026-04-04 14:38'
labels:
  - builders
  - ui
  - api
  - auth
dependencies: []
references:
  - packages/default/src/handlers/api/builders.rs
  - packages/default/src/queries/builders.rs
  - packages/web-ui/src/api/client.rs
  - packages/web-ui/src/views
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

Rotating or updating a builder key through the Crystal Forge UI does not reliably update the builder's `public_key` in the database.

Observed production impact:
- operator generated a new builder key through the UI
- new private key was installed on the builder host
- builder service restarted and derived a new public key from `/var/lib/crystal-forge/builder-api.key`
- server-side `builders.public_key` remained the old value
- all builder-authenticated API calls (`next-job`, `heartbeat`) failed with `401 Unauthorized`
- server logs showed: `builder auth rejected: signature verification failed`

This creates a high-severity outage for builders because key rotation appears to succeed from the UI/operator perspective, but the builder record on the server is left out of sync with the deployed private key.

## Goal

Make builder key update/rotation flows authoritative and consistent:
- when the UI/API updates or regenerates a builder key, the corresponding `builders.public_key` row must persist the matching public key
- the UI must not present key rotation as successful unless the DB update has actually succeeded

## Desired Outcome

After rotating/regenerating a builder key from the UI:
1. the server stores the new `public_key` for that builder row
2. the operator can deploy the matching private key to the builder host
3. builder-authenticated requests succeed immediately after restart

## Scope

Investigate and fix the full path for builder key updates:
- builder management UI action(s)
- frontend API client request
- backend handler(s) for update/regenerate key
- query layer that persists `builders.public_key`
- success/error handling in the UI so a failed update is obvious

Likely files:
- `packages/web-ui/src/views/...` builder management page
- `packages/web-ui/src/api/client.rs`
- `packages/default/src/handlers/api/builders.rs`
- `packages/default/src/queries/builders.rs`
- `packages/default/src/models/builders.rs`

## Non-Goals

- No redesign of builder authentication protocol
- No change to the on-host key deployment mechanism itself
- No builder service/systemd config changes

## Verification Plan

### Tier 0
- `nix develop -c env SQLX_OFFLINE=true cargo check --package crystal-forge`
- `nix develop -c cargo check --package crystal-forge-ui`
- targeted tests for backend key update handler/query

### Tier 1
- Create or rotate a builder key from the UI
- verify `builders.public_key` changes in DB immediately
- deploy matching private key to a builder host
- restart builder and confirm heartbeat / next-job succeed without 401

## Risk Level

High

This affects builder authentication and can take builders fully offline if broken.

## References

- `packages/default/src/handlers/api/builders.rs`
- `packages/default/src/queries/builders.rs`
- builder management UI in `packages/web-ui`
- incident observed on builder `reckless-builder` (`5c9cf001-118a-42b2-9efd-92522efe594a`)
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Rotating or updating a builder key via the UI persists the matching new `public_key` to the `builders` table.
- [ ] #2 The UI shows a clear error if the DB update fails and must not imply success on partial failure.
- [ ] #3 A builder restarted with the matching private key can authenticate successfully after rotation (heartbeat and next-job no longer 401).
- [ ] #4 Targeted backend and UI verification covers the key update path end-to-end.
<!-- AC:END -->

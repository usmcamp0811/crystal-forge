---
id: TASK-435
title: Implement System Agent Key Rotation from the Agent Identity Section
status: To Do
assignee: []
created_date: '2026-08-25 03:12'
labels:
  - web-ui
  - server
  - auth
  - systems
  - security
dependencies: []
references:
  - packages/web-ui/src/components/system/edit_system_modal.rs
  - packages/web-ui/src/components/modals/update_public_key_modal.rs
  - packages/web-ui/src/components/modals/key_pair_modal.rs
  - packages/web-ui/src/views/systems_list.rs
  - packages/web-ui/src/views/systems_list_helpers.rs
  - packages/web-ui/src/systems/adapter.rs
  - packages/web-ui/src/api/client.rs
  - packages/default/crates/cf-server/src/handlers/api/systems.rs
  - packages/default/crates/cf-server/src/handlers/agent_request.rs
  - packages/default/crates/cf-server/src/models/public_key.rs
  - packages/default/crates/cf-server/src/queries/systems.rs
  - packages/default/crates/cf-server/src/auth/models.rs
  - docs/design/CrystalForge/components/EditSystemModal.jsx
  - checks/web-ui/coverage-manifest.json
  - checks/web-ui/tests/integration-test.js
  - task-244
documentation:
  - docs/design/CrystalForge/components/EditSystemModal.jsx
priority: high
type: feature
ordinal: 444000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Summary
`EditSystemModal` Security tab → "Agent identity" tells the operator rotation is unavailable and to use the Systems-view flow instead. That flow already exists and is backend-backed (`PUT /systems/:id/public-key`), but it only accepts a *pasted* public key — no generation step — and isn't reachable from System details. Crystal Forge already has a working client-side Ed25519 keygen (used only by "Add system"). This task wires the two together: a real "Rotate key" action in Agent Identity that reuses the existing generation function and existing persistence endpoint.

## Current behavior
`packages/web-ui/src/components/system/edit_system_modal.rs` Security tab (~L528-551) renders a `sd-callout-warning` saying rotation is unavailable + help text pointing at the Systems view. No key data/fingerprint shown; no action available.
The only backend-backed rotation entry point is the Systems-list row action **"Update Key"** (`system_card.rs:109-113` → `on_update_key` → `update_key_for_system` in `systems_list_helpers.rs:24-37` → `pending_update_key` in `systems_list.rs`) opening `UpdatePublicKeyModal` (`components/modals/update_public_key_modal.rs`): a paste-only textarea + length≥32 sanity check. It never generates a key.
`SystemDetail`/`SystemSummary` (`api/models.rs`) expose **no public key or fingerprint field** today, so nothing can display "current key" state anywhere yet.

## Existing authoritative key workflow (reuse, don't reimplement)
**Generation (authoritative for systems/agents):** `generate_key_pair()` in `components/modals/key_pair_modal.rs:17-45` — Ed25519 keypair generated in-browser (WASM) via `ed25519_dalek::SigningKey` seeded from `window.crypto.getRandomValues` (JS `Math.random` fallback), base64-encoded (plain base64, no `ssh-ed25519` armor). `KeyPairModal` (same file) shows it once (public+private, copy affordance) and never sends the private key anywhere. Today wired ONLY into "Add system" (`systems_list.rs` `on_generate_keys` → "Use Public Key" fills the draft). This is the mechanism to reuse — no second WASM/ed25519 impl, no new server-side keygen for systems (that pattern exists only for *builders*, see Out of scope).
**Persistence (already fully backend-backed):** `update_system_public_key_via_api` (`systems/adapter.rs:282-299`) → `api::client::update_system_public_key` (`api/client.rs:701-707`) → `PUT /systems/:id/public-key` with `{public_key}`, via `send_json_with_csrf`. Handler `update_system_public_key` (`cf-server/src/handlers/api/systems.rs:1096-1171`): auth (403 if none) → `can_mutate_systems()` role check (Admin/Operator only, `auth/models.rs:37-39`) → empty-key 400 → load system (404) → `can_access_system_environment` (404 if denied) → `PublicKey::from_base64` validation (32 raw bytes, valid Ed25519 point; 400 on failure) → `queries::systems::update_public_key` (single atomic `UPDATE systems SET public_key=$1, updated_at=NOW() WHERE id=$2`, no history table) → audit write via `record_system_mutation_audit` using `AuditAction::UserUpdated` **with an inline comment admitting it's a placeholder for "SystemKeyRotated"** → `200 SystemMutationResponse`.

## Agent auth consequences (verified, deterministic, already tested)
`authenticate_agent_request_with_lookup` (`handlers/agent_request.rs:69-116`, tests L215-490): every agent request is signed (`X-Key-ID`/`X-Signature`) and verified fresh against the live `systems.public_key` column — no session/cache. Rotation takes effect on the agent's very next request. `cf-agent` is heartbeat/poll based (no WebSocket) so "disconnect" doesn't apply — the old key just gets 401 next call until the operator replaces the private key file on the host.

## Target behavior
Follow the interaction structure already in `docs/design/CrystalForge/components/EditSystemModal.jsx` L263-360 (`rotatingKey`/`keyMode`/`generatedKeys`/`rotated` state; tracked in `checks/web-ui/coverage-manifest.json` as `12e-systems-edit-modal`). Note: the design mock's `genKeypair()` fabricates OpenSSH-armored strings purely for the mock — use the real base64 format from `generate_key_pair()`/`PublicKey::from_base64`, not the mock's string shape.
1. Security tab shows current key fingerprint (new field) instead of the unavailable callout.
2. "Rotate key" reveals a Generate/Paste mode toggle (generate default).
3. Generate mode: button calls `generate_key_pair()`; shows public+private inline (private "shown once, copy it now").
4. Paste mode: reuses (shared, not duplicated) the existing paste textarea/validation from `UpdatePublicKeyModal`.
5. Persistent warning: old key revoked immediately, agent must present new key next heartbeat.
6. Destructive-styled confirm, disabled until a valid key exists, calls the same `update_system_public_key_via_api`.
7. Success: fingerprint updates in place (no reload). Failure: show error, **keep the already-generated key on screen** for retry (don't force regeneration — operator may be mid-install of that exact private key).
8. Cancel before confirm leaves the stored key untouched. Placeholder text removed entirely.

## Architecture / implementation approach
Primarily frontend/workflow integration; two small additive backend changes.
**Backend:** (a) add `AuditAction::SystemKeyRotated` (`api/models.rs` enum ~L2792, mirrored in web-ui `api/models.rs` ~L3348), use it in `handlers/api/systems.rs:1152` instead of `UserUpdated`; update the exhaustive `action_to_str` matches in `handlers/api/systems.rs:1604` and `handlers/api/admin.rs:964`, plus the admin UI label map in `views/admin.rs:2598-2639`. (b) expose `public_key_fingerprint` on `SystemDetail` (`cf-server/api/models.rs:949` + web-ui mirror `api/models.rs:673`) via the existing `PublicKey::fingerprint()` — read-only, additive, no schema change. Decide (record as an implementation decision if changed) whether `SystemMutationResponse` returns the new fingerprint vs. the frontend re-fetching `GET /systems/:id` after success — either satisfies "no reload".
**Frontend:** extract the paste-mode validation out of `UpdatePublicKeyModal` for shared use; extend `EditSystemModal` Security tab with the state machine above calling `generate_key_pair()`/`update_system_public_key_via_api` directly; keep `UpdatePublicKeyModal` and the Systems-list row action working unchanged (regression). No changes to `cf-keygen` or `cf-agent`.

## API / data-integrity / authorization / audit
`PUT /systems/:id/public-key` `{public_key}` → `200 {status:"success", message}`. Existing, must-preserve failure semantics: empty key 400; invalid key 400; unauthenticated 403; non-mutator role 403; not-found/no-env-access 404; DB failure 500; audit-write failure after a successful DB write is 500 even though **the key is already changed** (pre-existing behavior — call out, don't silently mask). Single-row UPDATE is already atomic. Confirm control must be disabled while in flight (duplicate-submit guard); generation itself never touches the server, so repeated regeneration before confirm has no server effect. Authorization: Admin or Operator with environment access (`can_mutate_systems` + `can_access_system_environment`), already enforced server-side — UI may hide the action for Viewers but must not be the sole guard. Audit: every success emits `SystemKeyRotated` via the existing `record_system_mutation_audit` path with unchanged target/metadata shape; never include private-key material.

## UX states
Initial (fingerprint + Rotate button) → mode select (Generate/Paste) → generating/pasting → key ready (confirm enabled) → confirming (disabled, loading) → success (fingerprint updates, private key gone) → error (key material retained, retry enabled) → cancel (no change) → reopen-after-close (new fingerprint shown as current; rotated-out private key never retrievable again).

## Error cases
Empty/invalid key (400), 401/403, 404 (system removed/reassigned mid-flow), 500 on DB write (no change, must not claim success), 500 on audit write after successful DB write (key IS changed — pre-existing, document not silently fix), network failure (unknown state, allow retry without losing generated key), operator closes before copying private key post-success (unrecoverable by design, must be made unmistakable pre-confirm).

## Out of scope
`cf-keygen`/`cf-agent` auth mechanics; the builder key-rotation subsystem (`regenerate_builder_keypair`, `handlers/api/builders.rs:1497-1532`, tracked by TASK-244 — different resource/pattern, do not port or conflate); general SSH/enrollment redesign or key history/versioning; broader System-modal redesign beyond Agent Identity; any new mutation endpoint (none needed).

## Verification plan
Tier 0: `nix develop -c env SQLX_OFFLINE=true cargo check --package cf-server`; `cargo check` for the web-ui crate.
Tier 1: extend `handlers::api::systems::tests` (`handlers/api/systems.rs:2397+`) with authorization (Viewer denied, out-of-env Operator denied), validation, success-path persistence, and `action_to_str` tests for the new variant; extend `handlers/agent_request.rs` tests with an old-key-rejected/new-key-accepted rotation regression; web-ui unit tests for extracted paste validation (pattern: `components/builders/edit_builder_modal_actions.rs`).
Tier 2: new scenario in `checks/web-ui/tests/integration-test.js` + `coverage-manifest.json` entry (e.g. `12e2-systems-edit-modal-key-rotation`, designRef `EditSystemModal.jsx`) opening Security tab on a seeded fixture system, generating, confirming, intercepting the `PUT .../public-key` request (pattern: existing `12e-systems-edit-modal` PATCH interception) to assert only the public key is sent and the fingerprint updates without reload; run via `nix build .#checks.x86_64-linux.web-ui -L`. Confirm the existing Systems-list "Update Key" scenario still passes unchanged.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The Agent identity section in EditSystemModal's Security tab provides a working Rotate key action (generate or paste), replacing the placeholder.
- [ ] #2 The 'SSH key rotation is unavailable in this modal…' callout and its 'use the Systems view' help text are removed from edit_system_modal.rs.
- [ ] #3 Rotation (both generate and paste modes) calls the existing update_system_public_key_via_api / PUT /systems/:id/public-key path — no new mutation endpoint is introduced.
- [ ] #4 Key generation reuses the existing generate_key_pair() (ed25519_dalek + browser CSPRNG) implementation — no second WASM/ed25519 keygen implementation is added anywhere in the frontend.
- [ ] #5 Paste-mode validation is shared/extracted rather than duplicated between UpdatePublicKeyModal and the new Agent Identity rotate flow.
- [ ] #6 A successful rotation persists the new public key for the correct system_id via the existing single-row UPDATE query.
- [ ] #7 After a successful rotation the modal shows the updated fingerprint without a full page reload (via response data or an in-place refetch).
- [ ] #8 The generated private key is displayed at most once, is never sent to the server, and cannot be retrieved again through this workflow or any other UI after the modal is closed.
- [ ] #9 A failed public-key persistence request is never presented to the operator as a successful rotation, and the already-generated key pair remains visible for retry instead of forcing regeneration.
- [ ] #10 The confirm/rotate control is disabled while a rotation request is in flight, preventing duplicate submissions from a single click sequence.
- [ ] #11 Cancelling before confirm leaves the system's stored public key, fingerprint display, and audit log unchanged.
- [ ] #12 A Viewer (or a non-Admin caller without environment membership) cannot rotate a system's key; the server-side 403/404 behavior already in update_system_public_key is preserved and covered by a test.
- [ ] #13 A regression test proves a request signed with the pre-rotation private key is rejected (401) immediately after the public key row is updated, using authenticate_agent_request_with_lookup.
- [ ] #14 A regression test proves a request signed with the newly generated private key is accepted after rotation.
- [ ] #15 Successful rotation writes an audit event using a dedicated AuditAction::SystemKeyRotated variant (not the UserUpdated placeholder), verified by a test on action_to_str.
- [ ] #16 The rotate/generate/paste/confirm interaction in the Security tab matches docs/design/CrystalForge/components/EditSystemModal.jsx in structure, copy tone, and state transitions (mode toggle, one-time private key display, destructive-styled confirm, success callout), using the real base64 key format rather than the mock's OpenSSH-formatted strings.
- [ ] #17 The existing Systems-list 'Update Key' row action (UpdatePublicKeyModal) continues to work unchanged after this change.
- [ ] #18 cargo check for cf-server and the web-ui crate pass; the relevant cf-server --lib tests pass; nix build .#checks.x86_64-linux.web-ui passes including a new scenario exercising the rotate-from-Agent-Identity flow.
<!-- AC:END -->

## Definition of Done
<!-- DOD:BEGIN -->
- [ ] #1 Implementation reuses generate_key_pair() and PUT /systems/:id/public-key end-to-end; no parallel key-generation or key-persistence logic exists.
- [ ] #2 SystemKeyRotated audit action is emitted server-side and covered by a test; no private-key material appears in logs, audit metadata, or URLs at any point.
- [ ] #3 cf-server --lib tests (authorization, validation, persistence, audit, and the old-key-rejected/new-key-accepted auth regression) pass.
- [ ] #4 nix build .#checks.x86_64-linux.web-ui passes with a new browser scenario covering the Agent Identity rotate flow, and the pre-existing Systems-list Update Key scenario still passes.
- [ ] #5 The Security tab's Agent Identity section matches the EditSystemModal.jsx design reference for this feature (mode toggle, one-time key display, destructive confirm styling, success state).
<!-- DOD:END -->

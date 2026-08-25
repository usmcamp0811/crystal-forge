---
id: TASK-435
title: Implement System Agent Key Rotation from the Agent Identity Section
status: In Progress
assignee:
  - opencode
created_date: '2026-08-25 03:12'
updated_date: '2026-08-25 15:29'
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
modified_files:
  - checks/server-regressions/default.nix
  - checks/web-ui/coverage-manifest.json
  - checks/web-ui/tests/integration-test.js
  - packages/default/crates/cf-server/src/api/models.rs
  - packages/default/crates/cf-server/src/handlers/agent_request.rs
  - packages/default/crates/cf-server/src/handlers/api/admin.rs
  - packages/default/crates/cf-server/src/handlers/api/systems.rs
  - packages/default/crates/cf-server/src/queries/systems.rs
  - packages/default/crates/cf-server/src/services/systems.rs
  - packages/web-ui/Cargo.lock
  - packages/web-ui/Cargo.toml
  - packages/web-ui/assets/app.css
  - packages/web-ui/src/api/models.rs
  - packages/web-ui/src/components/modals/update_public_key_modal.rs
  - packages/web-ui/src/components/system/edit_system_modal.rs
  - packages/web-ui/src/components/system/key_rotation.rs
  - packages/web-ui/src/components/system/mod.rs
  - packages/web-ui/src/systems/adapter.rs
  - packages/web-ui/src/views/admin.rs
  - packages/web-ui/src/views/systems_mock_data.rs
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
- [x] #2 The 'SSH key rotation is unavailable in this modal…' callout and its 'use the Systems view' help text are removed from edit_system_modal.rs.
- [x] #3 Rotation (both generate and paste modes) calls the existing update_system_public_key_via_api / PUT /systems/:id/public-key path — no new mutation endpoint is introduced.
- [x] #4 Key generation reuses the existing generate_key_pair() (ed25519_dalek + browser CSPRNG) implementation — no second WASM/ed25519 keygen implementation is added anywhere in the frontend.
- [x] #5 Paste-mode validation is shared/extracted rather than duplicated between UpdatePublicKeyModal and the new Agent Identity rotate flow.
- [x] #6 A successful rotation persists the new public key for the correct system_id via the existing single-row UPDATE query.
- [ ] #7 After a successful rotation the modal shows the updated fingerprint without a full page reload (via response data or an in-place refetch).
- [ ] #8 The generated private key is displayed at most once, is never sent to the server, and cannot be retrieved again through this workflow or any other UI after the modal is closed.
- [ ] #9 A failed public-key persistence request is never presented to the operator as a successful rotation, and the already-generated key pair remains visible for retry instead of forcing regeneration.
- [ ] #10 The confirm/rotate control is disabled while a rotation request is in flight, preventing duplicate submissions from a single click sequence.
- [ ] #11 Cancelling before confirm leaves the system's stored public key, fingerprint display, and audit log unchanged.
- [x] #12 A Viewer (or a non-Admin caller without environment membership) cannot rotate a system's key; the server-side 403/404 behavior already in update_system_public_key is preserved and covered by a test.
- [x] #13 A regression test proves a request signed with the pre-rotation private key is rejected (401) immediately after the public key row is updated, using authenticate_agent_request_with_lookup.
- [x] #14 A regression test proves a request signed with the newly generated private key is accepted after rotation.
- [x] #15 Successful rotation writes an audit event using a dedicated AuditAction::SystemKeyRotated variant (not the UserUpdated placeholder), verified by a test on action_to_str.
- [ ] #16 The rotate/generate/paste/confirm interaction in the Security tab matches docs/design/CrystalForge/components/EditSystemModal.jsx in structure, copy tone, and state transitions (mode toggle, one-time private key display, destructive-styled confirm, success callout), using the real base64 key format rather than the mock's OpenSSH-formatted strings.
- [ ] #17 The existing Systems-list 'Update Key' row action (UpdatePublicKeyModal) continues to work unchanged after this change.
- [ ] #18 cargo check for cf-server and the web-ui crate pass; the relevant cf-server --lib tests pass; nix build .#checks.x86_64-linux.web-ui passes including a new scenario exercising the rotate-from-Agent-Identity flow.
<!-- AC:END -->

## Definition of Done
<!-- DOD:BEGIN -->
- [x] #1 Implementation reuses generate_key_pair() and PUT /systems/:id/public-key end-to-end; no parallel key-generation or key-persistence logic exists.
- [x] #2 SystemKeyRotated audit action is emitted server-side and covered by a test; no private-key material appears in logs, audit metadata, or URLs at any point.
- [x] #3 cf-server --lib tests (authorization, validation, persistence, audit, and the old-key-rejected/new-key-accepted auth regression) pass.
- [ ] #4 nix build .#checks.x86_64-linux.web-ui passes with a new browser scenario covering the Agent Identity rotate flow, and the pre-existing Systems-list Update Key scenario still passes.
- [ ] #5 The Security tab's Agent Identity section matches the EditSystemModal.jsx design reference for this feature (mode toggle, one-time key display, destructive confirm styling, success state).
<!-- DOD:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
## Implementation plan (TASK-435)

Worktree: `/home/mcamp/code/crystal-forge/TASK-435-system-agent-key-rotation`, branch `TASK-435-system-agent-key-rotation` off `dev` (d5e4a19d).

### Decisions recorded
1. **Fingerprint is computed client-side in the UI** (new `sha2` direct dep on `crystal-forge-ui`; already present transitively via `ed25519-dalek`, already pinned at 0.10.9 in `packages/web-ui/Cargo.lock`). Rationale: the design reference shows a *live* "New fingerprint" preview for a pasted key before any request is sent, which no server response can provide. The same helper then satisfies AC#7 (in-place update after success) deterministically, without a refetch and without changing `SystemMutationResponse`. Format is byte-identical to `PublicKey::fingerprint()` (`SHA256:` + base64-nopad(sha256(raw 32 bytes))) and is unit-tested against a fixed key.
2. **Current fingerprint comes from the server** as a new read-only `SystemDetail.public_key_fingerprint` field (mirrors the existing `builders.public_key_fingerprint` precedent). No schema/migration change: `get_system_detail_by_id` joins `systems s` for `public_key`; the query is runtime-checked `query_as::<_, SystemDetailRow>`, so no SQLx offline metadata regeneration is required.
3. `SystemMutationResponse` is left unchanged (no new response field), per decision 1.

### Backend (cf-server)
- `api/models.rs`: add `AuditAction::SystemKeyRotated`; add `#[serde(default)] pub public_key_fingerprint: Option<String>` to `SystemDetail`.
- `queries/systems.rs`: add `public_key: Option<String>` to `SystemDetailRow`; change `get_system_detail_by_id` to `SELECT vsd.*, s.public_key FROM view_system_detail vsd JOIN systems s ON s.id = vsd.id WHERE vsd.id = $1`.
- `handlers/api/systems.rs`: map the fingerprint in `detail_row_to_api_model` via `PublicKey::from_base64(...).fingerprint()`; replace the `AuditAction::UserUpdated` placeholder in `update_system_public_key` with `AuditAction::SystemKeyRotated`; extend `action_to_str`.
- `services/systems.rs`: map the fingerprint in its `detail_row_to_api_model`.
- `handlers/api/admin.rs`: extend `action_to_str` and `parse_audit_action` (`"system_key_rotated"`).
- No changes to the endpoint contract, authz order, or error semantics of `update_system_public_key`. The pre-existing "audit-write failure after a successful DB write returns 500 while the key IS already rotated" behaviour is intentionally left as-is and documented in a code comment.

### Frontend (web-ui)
- `api/models.rs`: mirror `AuditAction::SystemKeyRotated`; add `public_key_fingerprint` to `SystemDetail`.
- `views/admin.rs`: add the new variant to the severity + label maps (`system.rotate_key`).
- **New** `components/system/key_rotation.rs` — pure, natively unit-tested logic shared by both surfaces:
  - `validate_public_key_input(&str) -> Result<String, String>` (trim, empty, base64 decode, 32-byte length) — the single shared paste validator (AC#5).
  - `public_key_fingerprint(&str) -> Option<String>` — server-identical SHA256 fingerprint.
- `components/modals/update_public_key_modal.rs`: switch its inline checks to `validate_public_key_input` (no behaviour regression for the Systems-list row action; AC#17).
- `components/system/edit_system_modal.rs`: replace the "unavailable" callout + Systems-view help text with the real Agent-identity flow mirroring `EditSystemModal.jsx` L263-360: current fingerprint → `Rotate key` → Generate/Paste segmented toggle → generate via the existing `generate_key_pair()` (no second keygen) with one-time private-key display + copy → destructive `Revoke old key & rotate` confirm (disabled until valid, disabled while in flight) → `update_system_public_key_via_api` → success callout with the fingerprint updated in place; on failure keep the key material and allow retry. Cancel resets local state only. Stable `data-testid` hooks added for the browser check.

### Verification
- `nix develop -c env SQLX_OFFLINE=true cargo check --package cf-server` (manifest `packages/default/Cargo.toml`).
- `nix develop -c cargo check --manifest-path packages/web-ui/Cargo.toml`.
- `nix develop -c cargo test --manifest-path packages/default/Cargo.toml --package cf-server --lib` (new: `action_to_str` for the new variant, unauthenticated 403 on `update_system_public_key`, rotation regression in `handlers::agent_request::tests` proving old-key 401 / new-key accepted via `MockSystemLookup`, fingerprint parity).
- `nix develop -c cargo test --manifest-path packages/web-ui/Cargo.toml` (shared validator + fingerprint helper).
- `cargo fmt --check` for both manifests.
- Per explicit user instruction, `nix build .#checks.x86_64-linux.web-ui` is **not** run locally; the new `12e2-systems-edit-modal-key-rotation` scenario + `coverage-manifest.json` entry are added and CI runs the check on the MR.

### Known adjacent risk to confirm on CI
`checks/web-ui/tests/integration-test.js` scenario `12e-systems-edit-modal` asserts the modal subtitle "Update system registration, flake assignment, and deployment policy." while the component has said "…flake assignment, deployment policy, and security settings." since commit e6dd5014 (TASK-394, 2026-07-20). This looks like a pre-existing stale selector, not something this task introduces. Plan: leave it untouched initially, confirm against the CI run, and only then decide (with the user) whether to fix the selector in this MR.

## Review remediation plan for !319

Review target: `34a1133d`; work continues in the existing clean worktree/branch.

1. **Canonical reconciliation and parent synchronization:** preserve structured public-key update errors. After every 200 and every ambiguous 5xx/network/deserialize outcome, GET the authoritative `SystemDetail` and compare its persisted fingerprint with the submitted key. Treat a matching fingerprint as rotated, a confirmed non-match after 200 as an error, and an unresolved/mismatched ambiguous outcome as unknown state with explicit operator guidance while retaining generated key material. Add an `on_key_rotated(SystemDetail)` callback so both Systems list and System Detail update their parent-owned detail without page reload.
2. **Whole-modal mutation lock:** while rotation is in flight, block backdrop dismissal, footer Cancel/Save, tab switching, rotation mode changes, and Escape/dismiss paths supported by this modal. Add stable test hooks where needed.
3. **Truthful clipboard state:** await `navigator.clipboard.writeText`; show `Copied` only on fulfillment and render an explicit error on rejection while preserving the private key.
4. **Fail-closed generation:** change the single authoritative `generate_key_pair()` to return `Result`, require Web Crypto on WASM, remove `Math.random`, propagate actionable errors in Add System and Edit System, and keep native compilation/testability through a deterministic seed-to-keypair helper.
5. **Validation parity:** run `ed25519_dalek::VerifyingKey::from_bytes()` in the shared frontend validator and replace arbitrary-byte valid fixtures with real deterministic Ed25519 keys.
6. **Entry-point regression:** expose the already-wired Update Key action in the currently rendered SystemCardV2/SystemsTable action surfaces, then add browser coverage for a valid PUT through `UpdatePublicKeyModal`.
7. **Browser hardening coverage:** extend the focused key-rotation scenario(s) for ambiguous committed-then-500 reconciliation, stalled PUT egress locking, clipboard resolve/reject, direct System Detail close/reopen without page reload and persisted fingerprint, secure-RNG failure, and the Systems-list Update Key flow.
8. **Verification:** run focused formatting, native/wasm checks, web-ui unit tests, relevant server tests, SQLx check if query shapes change, JS/manifest/diff checks. Per the prior user instruction, leave `nix build .#checks.x86_64-linux.web-ui` to MR CI, then inspect its result and screenshot before returning the task to Review.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
LOCK: opencode (claude-opus-5) on rift in /home/mcamp/code/crystal-forge/TASK-435-system-agent-key-rotation

MR opened: https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/319 (branch TASK-435-system-agent-key-rotation -> dev, commit 34a1133d). LOCK retained pending review.

Implementation decisions taken during execution:
1. Fingerprints are computed client-side (`components/system/key_rotation::public_key_fingerprint`), adding `sha2` as a direct web-ui dependency (already in packages/web-ui/Cargo.lock via ed25519-dalek; the only lockfile change is one added dependency edge). Reason: the design reference shows a live new-fingerprint preview for a pasted key *before* any request exists, which no server response can provide. The same helper then satisfies AC#7 in place, so `SystemMutationResponse` is unchanged and no refetch is needed.
2. `SystemDetail.public_key_fingerprint` is server-derived from `systems.public_key`. `view_system_detail` does not expose the key, so `get_system_detail_by_id` now joins `systems` for that one column. The query is runtime-checked (`query_as::<_, SystemDetailRow>`), so no SQLx offline metadata regeneration was required; `cargo sqlx prepare --workspace --check` passes, confirming no drift.
3. Added `.sd-callout-healthy` to `packages/web-ui/assets/app.css`. The design reference uses that class for the success state and it was not defined (only `-info`, `-warn`, `-danger` existed).
4. `queries::systems::tests::system_detail_query_does_not_derive_generation_from_store_path_regex` was updated: it pinned the exact detail SELECT text. The guard's real intent (no regex-derived generation, no independent generation lateral join) is preserved; the assertion now allows exactly one extra column, `s.public_key`, and documents why.

Bug found and fixed while writing the DB-backed tests: `queries::systems::insert_system` does not bind `System.id` — the database assigns it — so tests must use the returned row's id, not the id they constructed. The pre-existing ignored test `rollback_system_generation_updates_desired_target_to_store_path` has the same latent problem; it was not touched (out of scope). Worth a follow-up if the user wants it.

Pre-existing issue fixed with explicit user approval: `checks/web-ui/tests/integration-test.js` scenario `12e-systems-edit-modal` asserted the modal subtitle "Update system registration, flake assignment, and deployment policy." while `EditSystemModal` has read "...flake assignment, deployment policy, and security settings." since commit e6dd5014 (TASK-394, 2026-07-20). That is not a substring match, so `assertVisible` would time out. Selector updated to the current copy. dev's pipeline state could not be independently confirmed (glab returned 401 for the pipelines API).

Verification performed at commit 34a1133d:
- `nix develop -c cargo fmt --manifest-path packages/default/Cargo.toml --all -- --check` — pass.
- `rustfmt --edition 2024 --check` on all 8 changed web-ui files — pass. Crate-wide `cargo fmt --manifest-path packages/web-ui/Cargo.toml -- --check` reports 16 pre-existing diffs, all in files this task does not touch (policy_editor_modal.rs, environments/adapter.rs, compliance.rs, policies_api.rs, policies.rs); deliberately not reformatted.
- `nix develop -c env SQLX_OFFLINE=true cargo check --manifest-path packages/default/Cargo.toml --package cf-server` — pass.
- `nix develop -c cargo check --manifest-path packages/web-ui/Cargo.toml` — pass.
- `nix develop -c bash -c 'cd packages/web-ui && cargo check --target wasm32-unknown-unknown'` — pass, no new warnings from changed files.
- `SQLX_OFFLINE=true cargo test --package cf-server --lib` — 1181 passed, 0 failed, 381 ignored.
- `cargo test --package cf-server --lib system_key_rotation_ -- --ignored --test-threads=1` — 4 passed against the repository's isolated local dev PostgreSQL at 127.0.0.1:3042 (db-only process-compose instance; database was empty with no `_sqlx_migrations` table, migrations 0001-0232 applied non-destructively first; no shared/staging/production instance involved, default port 5432 not used).
- `cargo sqlx prepare --workspace --check` — pass.
- `nix develop -c cargo test --manifest-path packages/web-ui/Cargo.toml` — 199 passed, 0 failed, 1 ignored.
- `node --check checks/web-ui/tests/integration-test.js`, JSON parse of `coverage-manifest.json`, `git diff --check` — pass.

Note: an earlier `cargo test -p cf-server --lib` run showed 3 failures in `queries::cve_scans_tests` with `PoolTimedOut` / `relation "derivations" does not exist`. Those are environmental (unmigrated local database), unrelated to this task, and all pass after the migrations were applied.

NOT run locally, per the user's explicit instruction to leave it to CI: `nix build .#checks.x86_64-linux.web-ui`. AC#18's browser-check clause and DoD#4/#5 therefore remain unverified until the MR pipeline reports, and the MR screenshot for the new Security-tab state must come from that run.

Acceptance-criteria evidence status at Review time (deliberately conservative):
- PROVEN by automated tests / objective inspection, checked: #2 (string removed from edit_system_modal.rs), #3, #4, #5 (code structure + no second keygen/validator), #6, #12 (DB-backed authz + persistence tests, 4 passed), #13, #14 (agent_request rotation regressions, 2 passed), #15 (action_to_str test + DB audit-row assertion).
- NOT YET PROVEN, left unchecked because the only objective evidence is the new browser scenario `12e2-systems-edit-modal-key-rotation`, which was not run locally per the user's instruction: #1, #7, #8, #9, #10, #11, #16, #17, #18. These are implemented and compile/type-check on both native and wasm32, but per the finalization guide they must not be checked from code presence alone. They will be checked from the MR pipeline's web-ui check result.
- DoD #1/#2/#3 checked (proven by tests). DoD #4/#5 left unchecked pending the pipeline.

LOCK STATUS: awaiting review. Lock retained on /home/mcamp/code/crystal-forge/TASK-435-system-agent-key-rotation because MR !319 review feedback will be implemented in this same worktree. Worktree is clean at 34a1133d and matches origin.

Review changes requested on MR !319 at 34a1133d. Task moved from Review back to In Progress. Scope accepted from the user's review verdict: P1 ambiguous-state recovery, whole-modal in-flight lock, truthful async clipboard, canonical parent refresh, fail-closed CSPRNG; P2 validation parity, Systems-list Update Key browser regression, and expanded browser coverage. The existing worktree is clean and tracks origin at the reviewed commit.

Review remediation implemented in the existing TASK-435 worktree. Final read-only review found no correctness or security blockers. Residual low risk: browser coverage tests committed-then-500 reconciliation and unit-tests network-error classification, but does not simulate an actual browser network-abort followed by a confirming canonical GET.

Post-remediation verification passed: `cargo check --manifest-path packages/web-ui/Cargo.toml`; `cargo check --manifest-path packages/web-ui/Cargo.toml --target wasm32-unknown-unknown`; `cargo test --manifest-path packages/web-ui/Cargo.toml`; changed-file `rustfmt --edition 2024 --check`; `node --check checks/web-ui/tests/integration-test.js`; coverage-manifest JSON parse; and `git diff --check`. Per user instruction, `nix build .#checks.x86_64-linux.web-ui` remains delegated to MR CI, so browser behavior and screenshots remain pending.
<!-- SECTION:NOTES:END -->

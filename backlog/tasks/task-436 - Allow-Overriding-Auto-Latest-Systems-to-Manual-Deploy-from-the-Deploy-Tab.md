---
id: TASK-436
title: Allow Overriding Auto-Latest Systems to Manual Deploy from the Deploy Tab
status: To Do
assignee: []
created_date: '2026-08-25 03:19'
labels:
  - web-ui
  - server
  - systems
  - deploy
dependencies: []
references:
  - packages/web-ui/src/views/system_detail.rs
  - packages/web-ui/src/systems/adapter.rs
  - packages/web-ui/src/components/system/edit_system_modal.rs
  - packages/default/crates/cf-server/src/handlers/api/systems.rs
  - packages/default/crates/cf-server/src/queries/systems.rs
  - checks/web-ui/coverage-manifest.json
  - checks/web-ui/tests/integration-test.js
priority: medium
type: feature
ordinal: 445000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Summary
Deploying a specific commit to a system whose `deployment_policy` is `auto_latest` is currently always rejected by the backend, and the Deploy tab has no way to recover from that except manually switching the policy in a separate Edit modal tab and retrying. Add an inline "override and deploy anyway" recovery path: when this exact rejection happens, offer an explicit override action that (1) switches the system to `manual` deployment policy and (2) deploys the originally-selected commit, without leaving the Deploy tab.

## Current behavior (verified)
- `DeployTab` (`packages/web-ui/src/views/system_detail.rs`, `fn DeployTab` ~L2650) always renders an enabled Deploy button for the selected commit, gated only by role (`allow_mutations`) — there is **no client-side check of `system.deployment_policy`** before allowing the click, and `DeployGatePanel` (~L3315) actually renders "Approvals: pass" for `auto_latest` systems (it only special-cases `manual`), i.e. the gate panel gives a false-positive "passing" signal today.
- Clicking Deploy calls `on_deploy_commit` → `deploy_system_via_api(system_id, commit_sha)` (`systems/adapter.rs:302-314`) → `POST /systems/:id/deploy`.
- Backend handler `deploy_system` (`cf-server/src/handlers/api/systems.rs:1638-1720`): after auth/role/environment checks and commit validation, `if !matches!(row.deployment_policy.as_str(), "manual" | "pinned") { return bad_request("Manual deployment is not allowed for auto_latest systems"); }` (L1677-1680). This is the exact, only source of that message.
- The 400 body is `ApiError { error: "validation_error", message: "Manual deployment is not allowed for auto_latest systems" }`. `send_json_with_csrf`/`decode_api_error_message` extracts `.message`, so `deploy_system_via_api` returns `Err("Manual deployment is not allowed for auto_latest systems")`.
- `system_detail.rs` L1163-1180 catches this and sets `deploy_action_notice` to `("Deploy request failed for {hostname}: {error}", false)`, rendered as a red `sd-callout-danger` at L3277-3290 (commit mode) / equivalent generation-mode block — this is exactly the message quoted by the user, already reproduced verbatim today.
- Generation-rollback deploys (`on_deploy_generation` → `rollback_system_generation`, `handlers/api/systems.rs:1022-1094`) have **no such policy check at all** — this restriction is specific to commit-based deploy. Scope this task to the commit-deploy path only.

## Existing mechanism to switch deployment policy (must be reused/extended, not duplicated)
- The only existing UI to change `deployment_policy` is the segmented control in `EditSystemModal`'s Deployment tab (`components/system/edit_system_modal.rs:429-460`), which builds a full `UpdateSystemRequest` (see `handle_save`, L125-171) and calls `update_system_via_api` → `PATCH /systems/:id` → `update_system_handler` (`cf-server/src/handlers/api/systems.rs:742-884`).
- **Important data-integrity risk to design around**: `UpdateSystemRequest.environment`, `.flake_name`, and `.system_configuration_name` are plain `Option<String>` with **full-replace semantics server-side** — `update_system_handler` resolves `None`/empty to "clear" (`environment_id = None`, `flake_id = None`), and `queries::systems::update_system_metadata` (L163-222+) unconditionally writes whatever was resolved. Only `fqdn` and `heartbeat_interval_secs` are tri-state (`FieldUpdate::Unset` preserves them). **A naive "just send `deployment_policy: manual`" request would silently clear the system's environment and flake assignment.** Any implementation that reuses this endpoint must reconstruct the full request from the already-loaded `SystemDetail` (hostname, environment, flake.name, system_configuration_name) exactly as `EditSystemModal::handle_save` already does, and may use `FieldUpdate::Unset` for `fqdn`/`heartbeat_interval_secs` to preserve them untouched.
- `update_system_handler` currently writes **no audit event at all** (unlike `deploy_system`, which writes `AuditAction::SystemDeployRequested`). This is a pre-existing gap, not introduced by this task — call it out as an implementation decision rather than silently leaving a policy change unaudited.

## Target behavior
1. Operator selects a commit and clicks Deploy on an `auto_latest` system exactly as today; the existing rejection and red callout with the exact message above still appear unchanged.
2. Because `system.deployment_policy` (already loaded, no extra fetch needed) is `auto_latest`, the callout additionally offers an **"Override and deploy anyway"** action alongside the error, plus a short explanation that confirming will switch the system to manual deployment policy.
3. Confirming triggers, in order: (a) update the system's `deployment_policy` to `manual` via the existing PATCH path, preserving every other field exactly as loaded; (b) on success, immediately re-issue the deploy request for the same `commit_sha` via the existing `deploy_system_via_api`.
4. If step (a) fails, show that failure distinctly and do not attempt the deploy.
5. If step (a) succeeds but step (b) fails, show a message that makes clear the policy was already switched to manual even though the deploy itself failed (do not imply nothing happened).
6. On full success, the Deploy tab reflects the new `manual` policy (e.g. `policy_for_callout`/`DeployGatePanel` update) without a full page reload, and the normal deploy-accepted notice is shown.
7. The override control must be disabled while either sub-request is in flight, and must not be re-triggerable by repeated clicks mid-flight.
8. Systems already on `manual` or `pinned` never see this path — the existing deploy flow for those policies is unaffected.

## Architecture / implementation approach
Primarily frontend. Reuse existing endpoints:
- Reuse `update_system_via_api` (`systems/adapter.rs`) for the policy switch, constructing the request from the `system: SystemDetail` already passed into `DeployTab`, mirroring `EditSystemModal::handle_save`'s field construction (hostname, `fqdn: FieldUpdate::Unset`, `system_configuration_name`, `environment`, `flake_name`, `deployment_policy: "manual"`, `heartbeat_interval_secs: FieldUpdate::Unset`).
- Reuse `deploy_system_via_api` unchanged for the deploy step.
- Add the override UI/state machine inside `DeployTab` (or a small extracted helper) rather than a new modal component, consistent with the existing inline `deploy_notice` pattern.
- Backend: no new endpoint is strictly required. If the implementer determines the full-replace risk above is unacceptable even with careful reconstruction (e.g. because of a race with a concurrent edit), a narrow alternative is a dedicated `PATCH /systems/:id/deployment-policy` endpoint that only ever touches the `deployment_policy` column — this is an acceptable, smaller-footprint alternative and should be recorded as the approach taken either way. Whichever path is chosen, add an audit event for the policy change (there is none today) using the existing `record_system_mutation_audit` helper, distinct from `SystemDeployRequested`.

## Error cases
- Policy-switch request fails (403/404/500/network) — do not attempt deploy; system remains on its original policy.
- Policy-switch succeeds, deploy fails — system is now `manual`; message must say so explicitly, not just "deploy failed".
- System's policy or commit availability changes between load and override click (e.g. another operator already switched it, or the commit no longer belongs to the flake) — surface the server's actual error, do not assume success.
- Double-click / duplicate submission of the override button while in flight.

## Out of scope
- Generation-rollback deploys (no policy restriction exists there today).
- Any change to the `pinned` policy's existing allowed-deploy behavior.
- Broader redesign of `DeployGatePanel` beyond making it not falsely show "Approvals: pass" for `auto_latest` (fixing that false-positive is in scope only insofar as it's misleading next to the new override flow — do not redesign the gate panel's other rules).
- Changing what `auto_latest` means for automatic deployments themselves (out-of-band auto-deploy logic is untouched).

## Verification plan
Tier 0: `nix develop -c env SQLX_OFFLINE=true cargo check --package cf-server`; `cargo check` for the `crystal-forge-ui` package.
Tier 1: `cargo test --offline --package cf-server --lib` — add/extend tests around `deploy_system` and `update_system_handler` proving (a) the auto_latest rejection is unchanged, (b) a policy-only update via the chosen path does not clear `environment_id`/`flake_id`/`system_configuration_name`, (c) the new audit event is recorded for the policy change. Web-ui unit tests for the request-construction helper (pattern: `edit_builder_modal_actions.rs`-style pure-function tests) proving it preserves all current fields except `deployment_policy`.
Tier 2: extend/add a `checks/web-ui` scenario (`checks/web-ui/tests/integration-test.js` + `coverage-manifest.json`) that deploys a commit against a seeded `auto_latest` fixture system, asserts the existing failure message, clicks "Override and deploy anyway", intercepts both the PATCH and the deploy POST to assert payload correctness (policy=manual, other fields unchanged, correct commit), and asserts the tab reflects the new policy without reload. Run via `nix build .#checks.x86_64-linux.web-ui -L`.

## Risk Level
Medium — touches an existing deploy-rejection path and a system-mutation endpoint with full-replace semantics; the environment/flake-clobber risk must be explicitly tested, not just implemented.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Deploying a commit against an auto_latest system still produces the existing 'Deploy request failed for {hostname}: Manual deployment is not allowed for auto_latest systems' message unchanged.
- [ ] #2 When that specific failure occurs on an auto_latest system, the Deploy tab shows an 'Override and deploy anyway' action with an explanation that confirming switches the system to manual policy.
- [ ] #3 Confirming the override switches the system's deployment_policy to manual via the existing/extended update path, then deploys the same previously-selected commit.
- [ ] #4 The override does not clear or alter the system's environment, flake assignment, system_configuration_name, fqdn, or heartbeat_interval_secs.
- [ ] #5 If the policy switch fails, no deploy request is attempted and the system's policy is unchanged.
- [ ] #6 If the policy switch succeeds but the subsequent deploy fails, the UI message makes clear the policy was already changed to manual even though the deploy failed.
- [ ] #7 After a full successful override+deploy, the Deploy tab reflects the manual policy (including DeployGatePanel) without a full page reload.
- [ ] #8 The override control is disabled while a policy-switch or deploy request is in flight, preventing duplicate submissions.
- [ ] #9 Systems already on manual or pinned policy are unaffected by this change and never show the override action.
- [ ] #10 Generation-rollback deploys are unaffected by this change.
- [ ] #11 A dedicated audit event is recorded when a system's deployment policy is changed via this flow.
- [ ] #12 cargo check for cf-server and the web-ui crate pass; relevant cf-server --lib tests pass including a test proving environment/flake/system_configuration_name survive a policy-only override update; nix build .#checks.x86_64-linux.web-ui passes including a new scenario for this flow.
<!-- AC:END -->

## Definition of Done
<!-- DOD:BEGIN -->
- [ ] #1 A test explicitly proves the override path never clears environment_id, flake_id, or system_configuration_name for the target system.
- [ ] #2 A test proves the deploy-rejection message and conditions for auto_latest systems are unchanged for operators who do not use the override.
- [ ] #3 The policy-change audit event is verified by a test.
<!-- DOD:END -->

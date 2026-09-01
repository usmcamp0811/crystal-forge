---
id: TASK-437
title: >-
  Fix auto_latest target selection and system behind status to use newest
  deployable derivation
status: To Do
assignee: []
created_date: '2026-08-25 17:10'
updated_date: '2026-08-25 17:10'
labels:
  - deployment
  - bug
  - database
  - backend
dependencies: []
priority: high
type: bug
ordinal: 446000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## User-visible problem

Two related correctness defects in how Crystal Forge defines "latest":

1. Systems with `deployment_policy = 'auto_latest'` stay on an old generation even after a newer commit evaluates, builds, and cache-pushes successfully — unless an operator flips the system to `manual` and deploys once, after which heartbeats work again.
2. Systems correctly running the newest successful build are reported **behind** because a newer commit failed to evaluate/build or never became deployable.

## Confirmed technical findings (verified on dev @ 4c0f0575)

**Finding 1 — auto_latest selects derivations from only the single newest commit.**
`packages/default/crates/cf-server/src/queries/derivations.rs:2286` `get_latest_deployable_targets_for_flake_hosts()` uses `WITH latest_commit AS (... ORDER BY commit_timestamp DESC LIMIT 1)` joined to derivations, requiring `derivation_type='nixos' AND derivation_target IS NOT NULL`, completed cache push, agent/policy flags. If the absolute newest commit has no deployable derivation for a host (eval-failed, build-failed, still building, host absent, policy-failed, cache push incomplete), it returns nothing even though older commits have fully deployable derivations. Sole caller: `DeploymentPolicyManager::update_flake_systems_to_latest` (`src/deployment/mod.rs:207`) logs `"No deployable nixos derivation on latest commit"` and skips — `desired_target` never refreshes. Explains symptom 1.

**Finding 2 — `derivation_target IS NOT NULL` is not canonical.**
Manual resolution (`RESOLVE_SYSTEM_DEPLOYMENT_TARGET_SQL`, `src/queries/systems.rs:10-31`) requires: nixos type, `derivation_name = COALESCE(NULLIF(s.system_configuration_name,''), s.hostname)`, non-empty trimmed `store_path`, `cf_agent_enabled IS TRUE`, `policy_requirements_met IS TRUE`, EXISTS completed cache_push_jobs — NO `derivation_target` requirement. Unify on this set (proven correct by symptom 1 steps 5-6).

**Finding 3 — "behind" compares against absolute newest commit (same semantic bug, separate implementation).**
`view_system_deployment_status` is defined by migration `0106_fix_deployment_status_use_expected_store_path.sql` (last definition; later migrations only consume it). Its `latest_flake_commits` CTE takes the newest commit per hostname with no deployability filter; its CASE marks `'behind'` when `current_commit_id != lfc.latest_commit_id AND current_commit_timestamp < lfc.latest_commit_timestamp`. A failed newer commit makes a system on the newest good build report behind. Both `view_system_list` and `view_system_detail` project deployment_status from it (migration 0153); they agree, wrongly. Web UI only renders the value (`web-ui/src/views/system_detail.rs`).

**Finding 4 — heartbeat delivery and target discovery are separate.**
Discovery: the `DeploymentPolicyManager` poll loop (`deployment/mod.rs::run`, interval `config.deployment.deployment_poll_interval`) is the ONLY component refreshing `systems.desired_target` for auto_latest (`update_desired_target`, src/queries/deployment.rs). Delivery: `handlers/agent/heartbeat.rs:307` reads `desired_target` via `get_agent_desired_target_by_hostname` and returns it in LogResponse. Manual/pinned targets are set via `update_system_desired_target_with_source` (`manual_deploy`), resolving commit sha -> cached store path, rejecting uncached targets. Do not change delivery.

**Finding 5 — runtime gates are post-selection decisions, not artifact properties.**
After picking a target, the manager evaluates advanced policies (`evaluate_advanced_policy_gates`: time_window, require_approvals, canary_rollout, cve_threshold) plus legacy CVE gate; Pending/Block skips the desired_target update that iteration. They gate "may desired_target advance NOW", not deployability. Preserve this distinction.

**History (context only):** introduced aa32ff80 (2025-09); carried through 5b264413 (2025-11); TASK-242 (c054716d) fixed wiring but kept latest-commit-only selection. Longstanding latent defect; no existing query implements correct per-host-across-commits selection.

## Canonical semantic contract (authoritative)

**Latest deployable target for a system**: among the system's flake commits, the derivation with the newest commit (`commits.commit_timestamp DESC`, deterministic tie-break e.g. `derivations.id DESC`) satisfying ALL of: flake match; nixos type; `derivation_name = COALESCE(NULLIF(s.system_configuration_name,''), s.hostname)` (matches `System::configuration_name()`); evaluation succeeded; build succeeded with valid non-empty trimmed `store_path`; `cf_agent_enabled IS TRUE`; `policy_requirements_met IS TRUE`; EXISTS cache_push_jobs row with status='completed'. This is exactly the manual-resolution predicate extended across all commits. Runtime gates are NOT part of this definition.

## Required behavior

- A deployed + B newer built/cached => target=B; behind until installed; up_to_date after.
- B eval/build failed => target stays A; up_to_date; auto_latest never attempts B.
- B still building/cache-pending => A unmasked; when B completes cache push, target advances to B.
- B lacks this system's config derivation => host-specific target stays A.
- B has `policy_requirements_met=false` => target stays A. Runtime-gate blocks do not change which artifact is latest-deployable; they only pause advancement.
- No deployable derivation ever => NOT up_to_date; keep existing states (`no_deployment`/`unknown`/`no_commits`).
- `ahead` state keeps working.

## Implementation guidance

1. Rewrite `get_latest_deployable_targets_for_flake_hosts`: select per config name the newest derivation across ALL flake commits meeting the canonical contract; drop the LIMIT 1 commit CTE; drop `derivation_target IS NOT NULL`; keep completed-cache-push requirement; order by `commit_timestamp DESC, d.completed_at DESC NULLS LAST, d.id DESC`.
2. Fix `view_system_deployment_status` via a NEW migration following repo conventions (append-only CREATE OR REPLACE or 0153-style DROP+CREATE full column set): classify up_to_date/behind/no_deployment against the newest deployable derivation per system configuration, not raw HEAD. Update `status_description` truthfully ("Behind by N commits" only if accurate).
3. Keep DeploymentPolicyManager flow intact; improve logs to distinguish: no deployable build exists / newer commit not yet deployable / runtime gate blocked / already at target.
4. Do NOT touch heartbeat delivery, manual resolution semantics, agent code, or UI rendering.
5. SQLx: refresh .sqlx metadata against the repo-managed isolated local dev database only (docs/agents/database-safety.md). RESOLVE_SYSTEM_DEPLOYMENT_TARGET_SQL is runtime-checked, so unifying predicates there needs no metadata work unless shapes change.
6. Tests use the established DB-backed pattern `test_pool_from_env()` requiring DATABASE_URL (src/queries/systems.rs tests, TASK-258 pattern) plus include_str! migration-content assertions used in that module.

## Likely affected files

packages/default/crates/cf-server/src/queries/derivations.rs; packages/default/crates/cf-server/src/queries/systems.rs (shared predicate/tests); packages/default/crates/cf-server/src/deployment/mod.rs (logging only); new migration for view_system_deployment_status (+ list/detail if shape changes); .sqlx metadata; docs/onboarding-guide.md lines 207/641 ("deploy the latest evaluated commit" — wrong); docs/deployments_design_doc.md line 61 already correct. web-ui: none expected.

## Regression-test matrix (DB-backed)

Selection query: newest-commit-deployable selected; newer commit failed-eval / failed-build / build-pending / cache-push pending+failed / host-missing / policy_requirements_met=false each fall back to previous eligible target; multiple systems sharing one flake independently select their own newest deployable; config name differing from hostname resolves correctly; equal timestamps resolve deterministically; valid cached target not excluded due to irrelevant nullable metadata; no deployable derivation returns empty; once a newer derivation becomes deployable, selection advances.

Status view/APIs: running A + failed newer B => up_to_date; running A + pending newer B => up_to_date; running A + successful cached newer B => behind; running B => up_to_date; never-deployed => no_deployment; list and detail APIs agree.

Integration: system on A -> B built+cache-published -> update_auto_latest_policies iteration -> desired_target=B.store_path -> heartbeat exposes B -> deployment workflow can advance.

## Validation commands

- nix develop -c cargo test --manifest-path packages/default/Cargo.toml (targeted first)
- cargo sqlx prepare check after migration/query changes
- nix build .#packages.x86_64-linux.web-ui --no-link && nix build .#checks.x86_64-linux.web-ui --no-link (if UI surfaces touched)
- nix flake check --keep-going only if cross-package interfaces change

## Risks and edge cases

View migrations repeatedly broke consumers via column order (TASK-225); follow append-only CREATE OR REPLACE or 0153-style DROP+CREATE with include_str! assertions. Equal timestamps need deterministic tie-break. Derivations get re-inserted/upserted; tolerate multiple rows per (commit, name). All-commits selection must index well on derivations(commit_id)/cache_push_jobs(derivation_id,status). Policy conflict/failure currently skips systems entirely — preserve. Do not collapse runtime-gate state into deployability.

## Non-goals

No redesign of policies/gates; no heartbeat frequency changes; no binary-cache architecture changes; no Systems UI redesign; no manual/pinned behavior changes (already correct); no unrelated scheduling changes.

## Dependencies

None external. Requires repo-managed isolated local dev Postgres for SQLx prepare and DB-backed tests (docs/agents/database-safety.md).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A documented canonical definition of "latest deployable target" exists (code comment or task notes) matching RESOLVE_SYSTEM_DEPLOYMENT_TARGET_SQL prerequisites plus newest-across-commits selection.
- [ ] #2 get_latest_deployable_targets_for_flake_hosts returns, per requested configuration name, the newest derivation across ALL commits of the flake satisfying: nixos type, cf_agent_enabled IS TRUE, policy_requirements_met IS TRUE, non-empty trimmed store_path, EXISTS completed cache push job.
- [ ] #3 The derivation_target IS NOT NULL restriction is removed or replaced with the canonical requirements; auto-latest selection and manual resolution enforce identical artifact eligibility.
- [ ] #4 Given commits A<B where B has a failed evaluation, B has a failed build, B is still building, B lacks the host's derivation, B has policy_requirements_met=false, or B has no completed cache push: the query returns A's derivation for that host and DeploymentPolicyManager leaves desired_target pointing at A.store_path.
- [ ] #5 Given commits A<B where A is deployed and B becomes built with a completed cache push while policy is auto_latest: within one deployment_poll_interval iteration desired_target equals B.store_path with no policy change, and the next heartbeat LogResponse.desired_target returns B.store_path.
- [ ] #6 Runtime gates (approvals/canary/time_window/cve_threshold/legacy CVE) still run AFTER target selection and Pending/Block prevents the desired_target update; artifact selection itself does not evaluate these gates.
- [ ] #7 view_system_deployment_status reports up_to_date when the system's current store path maps to the newest deployable derivation, even when newer commits exist that are failed/pending/not-cache-published/host-missing.
- [ ] #8 view_system_deployment_status reports behind when a newer deployable derivation exists for that system configuration and the system runs an older one; status_description no longer claims 'behind by N commits' against raw HEAD unless accurate.
- [ ] #9 A system with no system_state row remains no_deployment, and a system whose flake has no deployable derivation at all is never up_to_date; ahead state keeps working.
- [ ] #10 Systems list and detail APIs both source deployment_status from the corrected shared definition and return identical status for the same fixture data.
- [ ] #11 DB-backed regression tests (test_pool_from_env pattern) cover the full matrix including equal-timestamp deterministic ordering and advance-on-becoming-deployable.
- [ ] #12 Integration test proves: system on A -> B built+cache-published -> update_auto_latest_policies iteration -> desired_target=B.store_path -> heartbeat exposes B.
- [ ] #13 Status tests prove: running A + failed newer B => up_to_date; running A + pending newer B => up_to_date; running A + successful cached newer B => behind; running B => up_to_date.
- [ ] #14 List and detail API paths tested so they cannot disagree.
- [ ] #15 Existing manual and pinned deployment behavior does not regress (existing resolve/deploy tests pass unchanged).
- [ ] #16 Operator logging distinguishes: no deployable build exists vs newer-not-yet-deployable commit exists vs runtime gate blocked vs already-at-target; message 'No deployable nixos derivation on latest commit' updated accordingly.
- [ ] #17 docs/onboarding-guide.md auto_latest wording corrected to 'automatically deploy the newest successfully deployable derivation'; behind-state semantics documented.
- [ ] #18 SQLx offline metadata updated if query shapes change; cargo sqlx prepare check passes.
- [ ] #19 nix develop -c cargo test --manifest-path packages/default/Cargo.toml passes; web-ui tests pass where affected; required Nix targets (nix build .#packages.x86_64-linux.web-ui --no-link, .#checks.x86_64-linux.web-ui --no-link) pass.
<!-- AC:END -->

## Definition of Done
<!-- DOD:BEGIN -->
- [ ] #1 One authoritative semantic definition of "latest deployable target" is documented in code/task notes.
- [ ] #2 auto_latest selects the newest deployable target across applicable commits, not only the absolute newest commit.
- [ ] #3 A newer failed/pending/non-applicable commit cannot mask an older deployable target.
- [ ] #4 A newly successful and cache-published target is automatically picked up without changing the deployment policy to manual.
- [ ] #5 desired_target is updated by the auto-latest manager when a newer deployable target becomes available, and normal heartbeat delivery propagates it to the agent.
- [ ] #6 System behind status is based on the newest deployable target for that system rather than merely the newest repository commit; a system on the last successful deployable build stays up_to_date when newer commits fail or are pending, and becomes behind when a newer deployable build exists.
- [ ] #7 Systems list and system detail report consistent deployment status; host/configuration-specific selection works when several systems share a flake.
- [ ] #8 Relevant policy and cache requirements remain enforced.
- [ ] #9 Database regression tests cover failed, pending, successful, cache-pending, policy-failed, and host-missing newer commits; integration coverage proves automatic convergence after a new build becomes deployable.
- [ ] #10 Existing manual and pinned deployment behavior does not regress.
- [ ] #11 Server tests pass; web UI tests pass where affected; required Nix build/check targets pass.
- [ ] #12 SQLx offline metadata is updated if query changes require it.
- [ ] #13 Documentation is updated where semantics changed or were previously ambiguous.
<!-- DOD:END -->

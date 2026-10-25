---
id: TASK-490.3
title: 'Access: administration and environment form persistence scenarios'
status: In Progress
assignee:
  - '@openai'
created_date: '2026-10-17 16:35'
updated_date: '2026-10-23 21:02'
labels:
  - testing
  - integration
  - luna
dependencies:
  - TASK-490.1
references:
  - docs/knowledge/testing/integration-coverage-map.md
  - docs/knowledge/ui/admin-console-and-login-views.md
  - docs/design/CrystalForge/components/AdminView.jsx
  - docs/design/CrystalForge/components/EnvironmentsView.jsx
modified_files:
  - checks/integration-access/scenarios/fragments/admin-forms.py
  - checks/integration-access/scenarios/fragments/admin-forms.cjs
  - checks/integration-access/scenarios/fragments/environment-forms.py
  - checks/integration-access/scenarios/fragments/environment-forms.cjs
parent_task_id: TASK-490
priority: high
type: task
ordinal: 515000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Owning check integration-access, contract access.configuration.admin/resources. This controller-only task covers production UI user/settings/environment forms with independent persistence and reload. Latest owner selected runnable coverage progress in the existing TASK-490 iteration; bounded Admin subset is now implemented and executed. Whole task remains In Progress.

Implemented paths: checks/integration-access/scenarios/fragments/admin-forms.py/.cjs. Coordinator appends them after the original sixteen cases in lifecycle-local-bootstrap.nix and maps seven browser phases to the same spec file. This uses one existing VM/database; each browser phase signs in independently, records responses/IDs, reloads, then returns to Python for SQL before the next mutation. No packet VM or new owner.

Passed cases: UI creation of design user u3 Dana Chen (email, username, first/last name, Operator role, empty explicit memberships verified); role edit to Viewer and reload; cancel both new-account and role/enablement edits with protected rows unchanged; disable persisted/reloaded plus one failed login401/no new session; retry-policy save; classification immediate enable save and marking/level save. All settings use independently authored facts. Requirements: approved AdminView.jsx workflows and existing task criteria. Source code locates controls/DTOs; current output does not approve its own correctness. Screenshots are semantic evidence, not visual parity.

Remaining paths: environment-forms.py/.cjs, environment create/edit/duplicate/cancel/delete and explicit nonempty environment assignments, plus accepted empty/pending/failed-save states. For a specified failed-save state use a bounded real server outage; no API mocks. Exact missing presentation/retention decisions block only those states (D3); they do not block accepted environment success/validation cases. No cache/builder/flake/system form scope expansion, OIDC or product/UI/schema/design changes.

Verified at clean c254b1c54e0f40adfaa9d05b5766cfeffae9bb89 through sh packages/ci/integration-owner-build.sh integration-access: exit0, 23/23 cases, VM executed newly. Original16 first and unchanged; seven Admin cases appended. VM driver50.72s, startup10.5s once, configured4GiB/4cores with peak memory unmeasured. Evidence /nix/store/nv2rpc4szk7525aa9fpb4289nz1hpbqv-vm-test-run-cf-it-access-lifecycle-local-bootstrap-run. The aggregate built its registered focused gate; no separate focused execution was needed.

Use existing case/expect/receipt/http/sql_json/run_browser helpers, same-scenario receipts and production controls. Shared harness/facts/composition/registration remain coordinator-owned. No cross-derivation live state or cookie transfer. Preserve correct failing assertions and link product defects; no product fix is authorized.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The coordinator composes the four named form fragments into lifecycle-local-bootstrap; no packet-specific VM derivation is added
- [ ] #2 UI user creation/role edits, classification and retry settings, and environment create/edit/delete are proven by exact database reads and reload; expected values are independently authored
- [ ] #3 Cancel leaves rows unchanged; duplicate environment validation retains draft and creates no row; accepted empty/pending/disabled-save states are exercised in the correct order
- [ ] #4 A real bounded write failure restores the server, persists nothing and retains the draft where accepted requirements define that state; unresolved component/state authority is recorded as blocked
- [x] #5 Disabling a user persists enabled=false and rejects that user's real sign-in with 401
- [x] #6 The coordinator executes the focused lifecycle-local-bootstrap gate containing these fragments and records exact revision, command, result and evidence
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Latest owner selects next runnable product coverage progress. Implement bounded accepted Admin form success subset after existing16Access cases: production UI user creation, role edit/reload, disable/login401 and cancel no mutation; classification/retry settings saves/reload if feasible. Use design identities not already created in .2, same-scenario receipts, independent browser login and DB before/after. No new VM/owner and no production changes. Do not claim complete .3: environment CRUD and specific unresolved failed-save/presentation states remain separate outstanding cases. Coordinator owns facts/composition/registration; fragment writer owns admin-forms.py/.cjs only. Execute actual normal Access CI path and preserve accurate failed assertion if a product defect emerges. TASK-490.7 remains next Delivery roles/proving work after this bounded controller progress; no blanket D1 blocker.

Owner's wrap-up selects dependency-ready accepted functional work. Extend only environment-forms.py/.cjs in the existing coordinator lifecycle with real Admin UI create/edit/delete, cancel and duplicate validation, explicit nonempty account-environment membership and independent SQL/reload checks. Preserve Admin23 and actor state; no product or design changes. Deferred visual states belong in the canonical punch list, not fabricated assertions. Coordinator composes and executes the normal Access owner after compatible shell/preferences work.

Owner explicitly authorized the substantiated environment-default correctness repair in this current worktree/MR. Confirm actual POST201 then PATCH/policies500 and disabled legacy UUID against authoritative data and approved EnvironmentsView new draft at6e4ea520. New drafts must have zero optional gates; global CF-agent invariant remains enforced separately. Preserve existing edit assignments, validate explicit stale/disabled/nonassignable selection against authoritative assignable policies with established validation response/no assignment side effects; do not silently discard it. No schema/policy redesign or generic multi-request transaction/recovery framework. Keep the partial-save boundary explicit. Add focused tests and existing Access regressions for exact request, one persisted environment, metadata/optional assignments, reload/reopen, valid optional assignment and invariant enforcement. Verify owned previews before production UI edits, then normal Access owner and before/after evidence; commit separately from mobile/header/501.

Owner explicitly authorized the confirmed environment policy picker correction: filter the loaded authoritative assignable catalog with the entered search and remove the arbitrary first-12 reachability restriction. Preserve selected IDs, authoritative validation, and loading/empty/error behavior. Add existing Access lifecycle assertions for a match beyond row12 and exact persisted selection; no API/schema or policy redesign. Coordinator applies in existing locked integration worktree/MR335.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
## Current bounded Admin result

Active coordinator openai in existing TASK-490.1-integration-foundation worktree, draft MR !335. Task In Progress. Only AC#5 and #6 checked: disabled account persistence/login401 and actual scenario execution. AC#1 requires all four fragments (environment fragments absent); #2 also requires environment CRUD/nonempty membership bindings; #3 needs environment validation and selected states; #4 needs accepted failure-state coverage. They remain unchecked.

Source c254b1c54e0f40adfaa9d05b5766cfeffae9bb89. sh packages/ci/integration-owner-build.sh integration-access exited0; 23 cases passed. First16 preserved, seven new Admin phases. Each phase has independent browser result/screenshot and SQL receipts including authored name/role/enablement, unchanged cancel snapshot, retry settings and classification. Disabled login401 has no new session. UI uses real APIs, no mocks or forced clicks/DOM changes. No visual parity claim.

First form run at24f82d4 passed original16 plus creation/edit/cancel/disable; retry locator exactness failed because wrapping label includes select option text. Corrected locator prefix; status/persistence/reload assertions retained. No product defect. First interrupted300s package build started no VM; metadata package verification changed an input identity and required the embedded rebuild. FinalVMdriver50.72s; two actual local VM runs while developing forms, one scenario and one VM per run. Memory4GiB configured, peak notmeasured.

Retained final evidence /nix/store/nv2rpc4szk7525aa9fpb4289nz1hpbqv-vm-test-run-cf-it-access-lifecycle-local-bootstrap-run and integration-evidence/integration-access/. Uploaded result https://gitlab.com/-/project/70402481/uploads/4edf40060b019ca9dbec691d6759ec11/result.json ; creation screenshot https://gitlab.com/-/project/70402481/uploads/eb29f2d768a7a11ce90222996bce7a10/admin-form-create-reloaded.png ; classification screenshot https://gitlab.com/-/project/70402481/uploads/f60cc53f8412e96c5550ad83a6763374/admin-form-classification-reloaded.png .

Remaining environment success/cancel/validation cases are controller-runnable. Exact unspecified failed-save/pending/empty presentation states need their own D3 decision. .7 is next shared-role/Delivery proving work, not blocked by historical foundation startup failures or blanket D1. No task completion/merge/deploy claim.

LOCK: openai on sledge in /home/mcamp/code/crystal-forge/TASK-490.1-integration-foundation.

## Historical
Earlier records at81d65be7 proved ten bootstrap/session cases, not forms, and task was then Backlog. .25/.26 prerequisites were integrated; selected role denials later produced sixteen cases at8a857bc8. Those readiness statements are superseded by this executed bounded slice.

Authorized policy picker correction is implemented locally in environment_form_modal.rs with controlled case-insensitive search over the full authoritative catalog, selected IDs retained and no first-12 limit. Worker focused tests passed3 filter regressions/24environment tests; real populated preview158catalog selected RequirePackages atzero-based29, PATCH200 exactUUID/reload/reopen/metadataedit and independentSQL passed (/tmp/opencode/cf-policy-picker-handoff.md). Normal Access uses its existing authored CustomCheck UUID...0003 plus12 unassigned custom_check catalog prerequisites named Access picker prerequisite01..12 before form creation; minassignable13 and independentSQL/browserorder assertions distinguish the old restriction without seeding a successful environment or assignment. This preserves the existing target and all six original form cases. Normal owner currently running, not passed. Preview ready http://127.0.0.1:8090/environments on sledge, same ownedAPI3490/PG5490; final durableMR and same-head proof pending.
<!-- SECTION:NOTES:END -->

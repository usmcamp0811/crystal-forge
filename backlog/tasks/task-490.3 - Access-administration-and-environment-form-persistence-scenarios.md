---
id: TASK-490.3
title: 'Access: administration and environment form persistence scenarios'
status: In Progress
assignee:
  - '@openai'
created_date: '2026-10-17 16:35'
updated_date: '2026-10-19 02:12'
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
<!-- SECTION:NOTES:END -->

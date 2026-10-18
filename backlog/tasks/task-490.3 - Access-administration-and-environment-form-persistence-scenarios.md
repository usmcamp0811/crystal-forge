---
id: TASK-490.3
title: 'Access: administration and environment form persistence scenarios'
status: Backlog
assignee: []
created_date: '2026-10-17 16:35'
updated_date: '2026-10-18 13:56'
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
  - checks/integration-access/scenarios/fragments/task-490.3-admin-forms.py
  - checks/integration-access/scenarios/fragments/task-490.3-admin-forms.cjs
  - >-
    checks/integration-access/scenarios/fragments/task-490.3-environment-forms.py
  - >-
    checks/integration-access/scenarios/fragments/task-490.3-environment-forms.cjs
parent_task_id: TASK-490
priority: high
type: task
ordinal: 515000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Owning check: integration-access. Contract IDs: access.configuration.admin, access.configuration.resources. Goal: prove real UI administration/environment persistence, reload, cancel and accepted failed-save behavior. Risk: medium. Non-goals: cache/builder/flake/system forms, cache credentials/Test (.12/.13), OIDC (.6), role enforcement (.2), unapproved visual baselines and product changes.

Writable paths (only):
- checks/integration-access/scenarios/fragments/task-490.3-admin-forms.py
- checks/integration-access/scenarios/fragments/task-490.3-admin-forms.cjs
- checks/integration-access/scenarios/fragments/task-490.3-environment-forms.py
- checks/integration-access/scenarios/fragments/task-490.3-environment-forms.cjs

Composition target: checks/integration-access/scenarios/lifecycle-local-bootstrap.nix. Coordinator owns Nix/Python composition, browserSpecs mapping and explicit owner registration/normal aggregation. Compose with .2/.4 after real first-admin bootstrap in one fresh local-auth VM/database. Reuse earlier real users/environments only through same-scenario receipts; never consume another derivation's live state. No packet-specific derivation; shared helpers, production, registration, CI, docs and design sources are read-only.

Admin cases: create user through UI and independently read users/user_role_assignments for exact role/environment assignments; edit role and reload; disable persists enabled=false and one sign-in attempt returns 401. Cancel leaves rows unchanged. Classification configuration and automatic retry policy saves persist and survive reload. Use authored values (design admin u1, Jordan Park u2/operator with production/staging where approved) and generated-ID receipts, not CF-derived expected values.

Environment cases: create/edit/delete through UI, verify environments rows and reload for each operation. Duplicate-name validation retains the draft and creates no row. Cover fresh-install empty list before these cases create records, success, validation, write error/draft preservation and pending/disabled save where accepted design defines those states. For failed-save behavior, use a real bounded server outage via unit_stopped(controller, "crystal-forge-server.service"), restore service and prove no persistence plus retained draft where approved. Missing authority blocks only the exact component/state; do not invent expectations or remove valid success/validation checks.

Authority/source reading: approved AdminView.jsx and EnvironmentsView.jsx under docs/design/CrystalForge/components are presentation references. docs/knowledge/ui/admin-console-and-login-views.md, api/builders-queues-environments-dashboard-admin-api.md and ui/flakes-environments-builds-and-evaluations-views.md are as-built evidence, not D1 acceptance authority. Read cf-server handlers/api/admin.rs (users, classification, retry DTOs), environments.rs and routes; web-ui admin/environment tabs and CreateUserModal, EditUserModal, EnvironmentFormModal, EditEnvironmentModal, RemoveEnvironmentDialog.

Dependency/readiness: TASK-490.1 controller interface is tested at bba718befbc183863781656df2df272e5f735d5a on origin/dev 5ae7feab, including .25/.26. Use documented case/expect/receipt/http named-session/CSRF/sql_json/run_browser and fault helpers. Sign in independently in each browser invocation. This packet is not selected and has not executed; coordinator must confirm accepted form/error states and composition before dispatch. Future services/builder/target interfaces are not prerequisites for controller form proofs.

Focused verification after composition: `nix build .#checks.x86_64-linux.integration-access.scenarios.lifecycle-local-bootstrap -L`. Record exact revision, command, outcome and evidence; a base bootstrap pass does not prove these fragments. Keep correct failing assertions and link product defects; escalate helper/authority gaps.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The coordinator composes the four named form fragments into lifecycle-local-bootstrap; no packet-specific VM derivation is added
- [ ] #2 UI user creation/role edits, classification and retry settings, and environment create/edit/delete are proven by exact database reads and reload; expected values are independently authored
- [ ] #3 Cancel leaves rows unchanged; duplicate environment validation retains draft and creates no row; accepted empty/pending/disabled-save states are exercised in the correct order
- [ ] #4 A real bounded write failure restores the server, persists nothing and retains the draft where accepted requirements define that state; unresolved component/state authority is recorded as blocked
- [ ] #5 Disabling a user persists enabled=false and rejects that user's real sign-in with 401
- [ ] #6 The coordinator executes the focused lifecycle-local-bootstrap gate containing these fragments and records exact revision, command, result and evidence
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Current readiness: controller bootstrap prerequisite bba718be is tested; this packet awaits user selection, approved form/error states and coordinator composition. No fragment result is claimed.

### Historical
2026-10-17 review recorded unresolved foundation/package blockers and no VM execution at that time. Separate form derivations were a former layout proposal. Bootstrap later passed at b1095c8f and after rebase at bba718befbc183863781656df2df272e5f735d5a on origin/dev 5ae7feab; runtime riky1mm0nn185wb26abw7zzshnbml0fp records 292 migrations. Those observations do not verify form fragments.
<!-- SECTION:NOTES:END -->

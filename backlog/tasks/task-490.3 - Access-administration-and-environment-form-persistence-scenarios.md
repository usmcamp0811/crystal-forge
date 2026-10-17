---
id: TASK-490.3
title: 'Access: administration and environment form persistence scenarios'
status: Backlog
assignee: []
created_date: '2026-10-17 16:35'
updated_date: '2026-10-17 17:25'
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
parent_task_id: TASK-490
priority: high
type: task
ordinal: 515000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Owning check: integration-access. Contract IDs: access.configuration.admin, access.configuration.resources. Mocked steps 14a-14c, 30c and 12e dominate existing persistence coverage; only 30b proves a real admin save. Prove browser action, authoritative effect and reload, plus failed-save draft preservation using real failures.

Exclusive writes: checks/integration-access/scenarios/configuration-admin-forms.{nix,py,cjs} and configuration-environment-forms.{nix,py,cjs}. Other paths are read-only. Non-goals: cache/builder/flake/system forms (domain setup), cache Test/credentials (490.12/.13), OIDC execution (490.6), role enforcement (490.2), unapproved visual baselines, production changes. TASK-490 test-only scope applies.

Corrected organization: owner default.nix at checks/integration-access/default.nix has Snowfall signature { lib, inputs, pkgs, ... }, explicit coordinator-owned scenarios, normal pkgs.linkFarm aggregate of ALL registered gates. Helpers: lib/integration-test via lib.crystal-forge.makeIntegrationTestHarness { pkgs, inputs }. Scripts/fixtures/assets are owner-local. No root tests/, legacyPackages.integration, mkOwner or custom discovery. Coordinator alone edits owner default.nix, shared lib, CI/docs and explicitly registers child scenarios. Meaningful WIP checks may be discoverable; only Access exists now, no placeholder owners.

Authority review: docs/knowledge/ui/admin-console-and-login-views.md, api/builders-queues-environments-dashboard-admin-api.md (admin/environments), ui/flakes-environments-builds-and-evaluations-views.md (Environments) are as-built evidence, not accepted D1 authority. Approved presentation references remain docs/design/CrystalForge/components/AdminView.jsx and EnvironmentsView.jsx, read-only. Uncertain behavior stays blocked pending owner decision; do not use Authority: as-built headers.

Read-only source: cf-server handlers/api/admin.rs (user list/create/update/delete, classification config and automatic retry policy, request DTOs), environments.rs (list/get/create/update/delete), server routes; web-ui admin.rs tabs and environments.rs, CreateUserModal, EditUserModal, EnvironmentFormModal, EditEnvironmentModal, RemoveEnvironmentDialog.

Verify actual foundation commit and harness signatures before dispatch. Pattern: checks/integration-access/scenarios/lifecycle-local-bootstrap.{nix,py,cjs}; controller browser role, API register/login setup and browser sign-in using the same credentials; helpers documented in coverage map. Authored/design facts: admin u1, u2 Jordan Park operator with production/staging, environments keyed by name (staging/lab); other values authored explicitly.

configuration-admin-forms: create user via Admin UI, verify users and user_role_assignments role/environments, reload; edit role persists; disable persists enabled=false and one login attempt is rejected (401); cancel leaves row unchanged; classification config and automatic retry policy save persist and survive reload. Inject a real write failure using unit_stopped(controller, crystal-forge-server.service) around save, restore service, assert draft retained and nothing persisted if accepted authority defines that failure state. Otherwise block pending owner/design decision with exact component/state.

configuration-environment-forms: UI create/edit/delete, each verified in environments and after reload. Duplicate-name validation retains draft and creates no row. Cover success, fresh-install empty list, write error/draft preservation, and pending/disabled save only where approved design defines it.

Candidate commands, NOT verified: nix build .#checks.x86_64-linux.integration-access.scenarios.configuration-admin-forms -L and configuration-environment-forms; append .run for evidence. Coordinator aggregate: nix build .#checks.x86_64-linux.integration-access -L. Run one VM test at a time until measured capacity supports a higher limit. Explicit registration and real VM execution required; notes record exact commands, revision, outcome and evidence paths. Independent expectations must come from authored facts/accepted requirements. Preserve correct failing assertions and linked Backlog product bugs; escalate harness/authority gaps to coordinator, never change product code or coordinator-owned files.

READINESS: foundation dependency commit unresolved; foundation blocked by two existing Rust package tests, no VM execution. Backlog, not ready. Future packet requires actual dependency commit(s), verified commands and implemented interfaces/fixtures/registration before To Do. No delegation or executed slice is claimed.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Both scenarios exist only under checks/integration-access/scenarios/ and are explicitly registered by the coordinator; the normal pkgs.linkFarm aggregate includes both gates
- [ ] #2 Each successful UI save is proven by a database read and reload read
- [ ] #3 Cancel leaves persisted rows unchanged
- [ ] #4 A real injected failure keeps the draft and persists nothing where accepted requirements define that state; uncertain failure behavior stays blocked pending owner decision with exact component/state
- [ ] #5 Disabling a user persists and prevents that user's sign-in
- [ ] #6 Both scenarios execute in real VMs with commands, revision, outcomes and .run evidence in task notes after actual dependency commits and verified commands are available
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
2026-10-17 organization correction: native owner-local paths, shared lib/integration-test and explicit coordinator registration replace the prior layout; D1 as-built docs are not authority. Foundation commit unresolved; two existing Rust package tests block foundation, no VM execution. Backlog, not dispatch-ready. Actual dependency commit(s), implemented interfaces and verified commands are required before To Do. No Luna delegation or slice execution is claimed.
<!-- SECTION:NOTES:END -->

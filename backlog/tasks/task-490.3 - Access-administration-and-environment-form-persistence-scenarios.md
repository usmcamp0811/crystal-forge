---
id: TASK-490.3
title: 'Access: administration and environment form persistence scenarios'
status: Backlog
assignee: []
created_date: '2026-10-17 16:35'
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
Owning check: integration-access. Contract IDs: access.configuration.admin, access.configuration.resources.

WHY: form persistence is mostly covered by mocked browser steps (14a-14c, 30c, 12e). Only 30b proves a real admin save. The contract requires the browser action, the authoritative effect, and a subsequent read, plus failed-save draft preservation without a fabricated response.

OBJECTIVE: two scenarios under tests/integration/access/scenarios/: `configuration-admin-forms` (Admin users create/edit/disable, classification config, automatic retry policy) and `configuration-environment-forms` (environment create/edit/delete). Test-only.

NON-GOALS: cache, builder, flake, and system forms (their domain owners use them as setup; cache Test/credentials belong to Delivery TASK-490.12/490.13); OIDC mappings execution (TASK-490.6); role enforcement (TASK-490.2); visual baselines for states without an approved design state; production changes.

BASE: the foundation commit recorded in implementation notes (FOUNDATION_COMMIT). Pattern: tests/integration/access/scenarios/lifecycle-local-bootstrap.{nix,py,cjs}.

AUTHORITY: docs/knowledge/ui/admin-console-and-login-views.md; docs/knowledge/api/builders-queues-environments-dashboard-admin-api.md (admin and environments sections); docs/knowledge/ui/flakes-environments-builds-and-evaluations-views.md (Environments). Approved design references: docs/design/CrystalForge/components/AdminView.jsx and EnvironmentsView.jsx (presentation only). As-built docs: header `Authority: as-built (D1)`.

PRODUCTION SOURCE TO READ (read-only): cf-server src/handlers/api/admin.rs (list_users, create_user, update_user, delete_user, get/update_classification_config, get/update_automatic_retry_policy; CreateAdminUserRequest, UpdateAdminUserRequest); src/handlers/api/environments.rs (list/get/create/update/delete environment handlers); src/bin/server.rs router lines for /api/v1/admin/* and /api/v1/environments*; packages/web-ui/src/views/admin.rs (tabs users, roles, oidc, jobs, audit, server) and views/environments.rs plus their modal components (CreateUserModal, EditUserModal, EnvironmentFormModal, EditEnvironmentModal, RemoveEnvironmentDialog).

WRITABLE (exclusive): tests/integration/access/scenarios/configuration-admin-forms.{nix,py,cjs}, tests/integration/access/scenarios/configuration-environment-forms.{nix,py,cjs}. READ-ONLY: all other paths.

HARNESS: nodes.controller = harness.roles.controller { browser = true; }; first admin as API setup (register + login via http() session) then sign in through the browser with the same credentials in the spec; helpers per docs/knowledge/testing/integration-coverage-map.md#harness-interface. Facts: admin u1, created user u2 (Jordan Park, operator, envs production+staging) via harness.facts.localUserFromDesign; environment names from the design fixture environment list keyed by name ("staging", "lab"); all other values authored in facts.

FAILURE INJECTION for draft preservation: use unit_stopped(controller, "crystal-forge-server.service") around the save click to produce a real failed write (not a mocked response), then restore and assert the draft is still in the form and nothing persisted. If the UI's behavior during a server outage is not specified by the approved design, record the case as `blocked: design gap` with the component and state instead of inventing an expectation.

SCENARIO configuration-admin-forms (actor: admin): create user through the Admin UI -> row in users + user_role_assignments with the authored role/environments; reload shows it; edit role -> persisted; disable -> enabled=false persisted and that user cannot sign in (one http() login attempt returns 401); cancel on an edited form leaves the row unchanged; classification config and retry policy save -> persisted and shown after reload; failed save keeps draft and persists nothing.
SCENARIO configuration-environment-forms (actor: admin): create, edit, delete an environment through the UI; each step verified in the environments table and after reload; validation error (e.g. duplicate name) keeps the draft and creates no row.
UI states to cover: success, loading during save (observe the disabled/pending state only if the design defines it), write error with draft preservation, empty list on a fresh install.

COMMANDS: nix build .#integration.access.scenarios.configuration-admin-forms -L ; nix build .#integration.access.scenarios.configuration-environment-forms -L ; append .run for evidence after failure. At most two VM tests concurrently. Coordinator runs .#integration.access.all.

COMPLETION, PRODUCT DEFECT, ESCALATION: same rules as TASK-490.2 (execute in real VMs; record commands, revision, outcome, evidence paths; failing correct assertions keep failing with a linked Backlog bug; interface gaps go to the coordinator; never edit coordinator-owned files).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 configuration-admin-forms and configuration-environment-forms exist only under tests/integration/access/scenarios/ and are discovered
- [ ] #2 Each successful UI save is proven by a database read and a reload read
- [ ] #3 Cancel leaves persisted rows unchanged
- [ ] #4 A real injected write failure keeps the draft and persists nothing, or the case is recorded as blocked: design gap with the exact component/state
- [ ] #5 Disabling a user prevents that user's sign-in
- [ ] #6 Both scenarios executed in real VMs with commands, revision, outcomes, and evidence paths in task notes
<!-- AC:END -->

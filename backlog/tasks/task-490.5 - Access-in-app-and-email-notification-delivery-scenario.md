---
id: TASK-490.5
title: 'Access: in-app and email notification delivery scenario'
status: Backlog
assignee: []
created_date: '2026-10-17 16:36'
updated_date: '2026-10-19 02:12'
labels:
  - testing
  - integration
  - luna
dependencies:
  - TASK-490.7
  - TASK-490.11
modified_files:
  - checks/integration-access/scenarios/fragments/notifications.py
  - checks/integration-access/scenarios/fragments/notifications.cjs
parent_task_id: TASK-490
priority: medium
type: task
ordinal: 517000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Owning check: integration-access. Contract ID: access.configuration.notifications. This packet owns checks/integration-access/scenarios/fragments/notifications.py and notifications.cjs, plus owner-local receiver fixture assets. The coordinator owns configuration-notifications.nix, Python composition, browserSpecs mapping, and owner registration. This is one scenario fragment packet, not a separate test derivation per packet.

Use a real BuildFailures or DeployFailures event and an email-enabled NixOS module configuration with the local receiver service. Verify in-app recipient filtering, read/read-all/dismiss/delete persistence, and email to subscribed recipients only; email-disabled produces none. Recreate the real event in this scenario. TASK-490.11 code/fixture dependencies do not supply another VM's database, session, or build output. Digest timing and external providers are out of scope.

Needs TASK-490.7 services/receiver/builder roles and TASK-490.11 event implementation, plus passing TASK-490.1 and TASK-490.25/.26. Read the notification APIs/worker and approved owner requirements; as-built docs are not authority. No application or coordinator-owned files. The coordinator registers/composes this scenario and records exact VM revision/command/evidence.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The coordinator composes the notification fragments into configuration-notifications; no packet-specific NixOS derivation is added
- [ ] #2 A real domain event creates exactly the expected in-app notifications per recipient subscription
- [ ] #3 Read/dismiss/delete persist through reload
- [ ] #4 Email reaches the local receiver only when enabled and only for subscribed users
- [ ] #5 The scenario's own VM creates its event and does not consume TASK-490.11 runtime state
- [ ] #6 The coordinator records actual VM revision, command, outcome and evidence
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
2026-10-17 organization correction: native owner-local layout and explicit coordinator registration replace prior paths. Foundation dependency commit unresolved; blocked by two existing Rust package tests, no VM execution. Backlog, not ready; future packet needs actual dependency commit(s) and verified commands before To Do. No Luna delegation or executed slice is claimed.

Isolation correction: TASK-490.11's failing-build runtime output cannot be reused. This scenario must run its own real failing build/event as setup in its own fresh VM/database, or consume only immutable fixture definitions/helpers from the integrated dependency. A task dependency never supplies another scenario's database, VM, session or output. Foundation dependency remains untested for successful server startup; remain Backlog.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
created: 2026-10-19 02:12
---
Source-path maintenance only: stable behavior-oriented filenames identify intended source ownership; the names do not prove implementation or execution. Existing requirements, status and recorded results remain unchanged.
---
<!-- COMMENTS:END -->

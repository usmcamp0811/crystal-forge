---
id: TASK-490.5
title: 'Access: in-app and email notification delivery scenario'
status: Backlog
assignee: []
created_date: '2026-10-17 16:36'
updated_date: '2026-10-17 17:26'
labels:
  - testing
  - integration
  - luna
dependencies:
  - TASK-490.7
  - TASK-490.11
parent_task_id: TASK-490
priority: medium
type: task
ordinal: 517000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Current packet boundary (2026-10-18): this task owns only notification Python/browser fragments under `checks/integration-access/scenarios/fragments/task-490.5-*`. The coordinator owns `configuration-notifications.nix`, composition, browserSpecs mapping, and registration. One separate fresh VM is justified by email-enabled module settings, local receiver service, and a real notification-producing build/deploy event. Recreate the event; TASK-490.11 runtime rows/output are not reusable. This packet does not define its own derivation.

Owning check: integration-access. Contract ID: access.configuration.notifications. Exclusive writes: checks/integration-access/scenarios/configuration-notifications.{nix,py,cjs}. Existing bell-feed coverage (09g-09i) is mocked; notifications must originate in a real domain event, not seeded rows.

Scope: a real BuildFailures (or DeployFailures) event from the Delivery fixture creates an in-app notification for a subscribed user and none for an unsubscribed user. Read, read-all, dismiss and delete persist. Enable server.notificationEmail toward the services-role local receiver: exactly the subscribed user's message arrives; email disabled produces none. Non-goals: digest timing and external providers. TASK-490 test-only restrictions apply.

Organization: native checks/integration-access/default.nix uses { lib, inputs, pkgs, ... }, explicit coordinator-owned scenarios and pkgs.linkFarm aggregating ALL registered gates. Shared helpers: lib/integration-test via lib.crystal-forge.makeIntegrationTestHarness { pkgs, inputs }. Scripts/fixtures/assets stay owner-local. No root tests/, legacyPackages.integration, mkOwner or custom discovery. Coordinator owns owner default.nix, shared lib and CI/docs; child integration needs explicit registration. Meaningful WIP may be discoverable; only Access exists now, no placeholder owners.

Prerequisites: TASK-490.7 services role/receiver/builder and TASK-490.11 real failing-build fixture. Read cf-server handlers/api/user_notifications.rs, models/user_notifications.rs, notification email worker, modules/nixos/crystal-forge server.notificationEmail.* and docs/knowledge/ui/alerts-and-notifications-decision.md. As-built docs are evidence, not accepted D1 authority; uncertain cases stay blocked pending owner decision.

Candidate, NOT verified: nix build .#checks.x86_64-linux.integration-access.scenarios.configuration-notifications -L; append .run for evidence. Coordinator aggregate: nix build .#checks.x86_64-linux.integration-access -L. Future packet must verify exact commands/interfaces on actual dependency commit(s). Completion requires real VM execution, independent authored expectations and notes with revision, commands, outcomes, evidence paths. Preserve correct failing assertions and linked Backlog bugs; escalate shared-interface/authority gaps without product fixes.

READINESS: actual foundation commit unresolved; foundation blocked by two existing Rust package tests; no VM execution. Remain Backlog until actual dependency commits, fixtures, registration and verified commands exist before To Do. No delegation or executed slice is claimed.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A real domain event produces exactly the expected in-app notifications per subscription
- [ ] #2 Read/dismiss/delete persist through reload
- [ ] #3 Email reaches the local receiver only when enabled and only for subscribed users
- [ ] #4 Executed in a real VM topology with recorded evidence
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
2026-10-17 organization correction: native owner-local layout and explicit coordinator registration replace prior paths. Foundation dependency commit unresolved; blocked by two existing Rust package tests, no VM execution. Backlog, not ready; future packet needs actual dependency commit(s) and verified commands before To Do. No Luna delegation or executed slice is claimed.
Scenario isolation: the TASK-490.11 failing-build run cannot supply notification rows/events. Generate a real failing build in this scenario's own fresh VM; reuse only immutable fixture definitions/helper code. Current TASK-490.1 VM fails on product defects TASK-490.25/.26; remain Backlog.
<!-- SECTION:NOTES:END -->

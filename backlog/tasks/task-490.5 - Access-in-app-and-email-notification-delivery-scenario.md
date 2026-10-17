---
id: TASK-490.5
title: 'Access: in-app and email notification delivery scenario'
status: Backlog
assignee: []
created_date: '2026-10-17 16:36'
updated_date: '2026-10-17 16:39'
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
Owning check: integration-access. Contract ID: access.configuration.notifications. Scenario: tests/integration/access/scenarios/configuration-notifications.{nix,py,cjs} (exclusive write path).

WHY: notification delivery is proven only by mocked bell-feed steps (09g-09i). A notification must come from a real domain event, not a seeded row.

SCOPE: a real BuildFailures (or DeployFailures) event produced through the Delivery fixture creates an in-app notification for a subscribed user and none for an unsubscribed user; read, read-all, dismiss, and delete persist; with server.notificationEmail enabled toward the local receiver on the services role, exactly the subscribed user's message arrives; with email disabled, none arrives. Non-goals: digest schedule timing; external providers.

READY WHEN: TASK-490.7 provides the services role with a notification receiver and the builder role, and TASK-490.11 provides a real failing-build fixture. Then the coordinator adds the base commit and moves this to To Do. Source to read then: cf-server src/handlers/api/user_notifications.rs, src/models/user_notifications.rs, notification email worker, modules/nixos/crystal-forge server.notificationEmail.* options; docs/knowledge/ui/alerts-and-notifications-decision.md.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A real domain event produces exactly the expected in-app notifications per subscription
- [ ] #2 Read/dismiss/delete persist through reload
- [ ] #3 Email reaches the local receiver only when enabled and only for subscribed users
- [ ] #4 Executed in a real VM topology with recorded evidence
<!-- AC:END -->

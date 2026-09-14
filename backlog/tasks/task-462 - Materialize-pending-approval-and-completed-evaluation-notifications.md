---
id: TASK-462
title: Materialize pending approval and completed evaluation notifications
status: Backlog
assignee: []
created_date: '2026-09-14 04:57'
labels:
  - notifications
  - backend
  - authorization
dependencies: []
references:
  - TASK-440
  - docs/design/CrystalForge/components/Shell.jsx
  - packages/default/crates/cf-server/src/queries/user_notifications.rs
priority: medium
type: feature
ordinal: 475000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Add production durable notification source events for pending deployment approvals and completed evaluations. The TASK-440 notification-center UI can route these events but current notification materialization does not emit them. Preserve account ownership current authorization non-disclosure deduplication preference cutoffs and durable read/dismiss state. This is event-semantic backend work and is outside the TASK-440 presentation parity slice.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A pending deployment approval creates one deduplicated durable notification for each currently authorized eligible account
- [ ] #2 The approval notification carries an authorized exact-system Deploy-tab target with a non-disclosing fallback when access changes
- [ ] #3 A completed evaluation creates one deduplicated durable notification for each currently authorized eligible account
- [ ] #4 Read and dismiss state remains independent from approval and evaluation lifecycle state
- [ ] #5 Current role environment and subject authorization is enforced during materialization list mutation and delivery
- [ ] #6 Focused PostgreSQL HTTP and browser tests prove deduplication routing authorization and durable inbox state
<!-- AC:END -->

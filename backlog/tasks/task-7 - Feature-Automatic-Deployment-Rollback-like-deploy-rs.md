---
id: TASK-7
title: 'Feature: Automatic Deployment Rollback (like deploy-rs)'
status: To Do
assignee: []
created_date: '2026-02-04 20:16'
labels:
  - feature
  - deployment
  - rollback
  - safety
dependencies:
  - TASK-1
priority: medium
milestone: m-4
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Add automatic rollback capability for failed deployments. Deploy new config, wait for health check/confirmation, auto-rollback if system doesn't respond or health check fails.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Design rollback mechanism (timeout vs health check vs manual)
- [ ] #2 Implement get_current_generation method
- [ ] #3 Implement rollback_to_generation method
- [ ] #4 Add RollbackConfig to deployment configuration
- [ ] #5 Implement timeout-based rollback
- [ ] #6 Implement health check framework
- [ ] #7 Add manual confirmation endpoint (server-side)
- [ ] #8 Test rollback on real NixOS system
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Future enhancement inspired by deploy-rs. Not required for initial deployment fix.
<!-- SECTION:NOTES:END -->

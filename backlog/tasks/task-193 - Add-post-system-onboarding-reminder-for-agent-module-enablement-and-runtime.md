---
id: TASK-193
title: Add post-system onboarding reminder for agent module enablement and runtime
status: Backlog
assignee: []
created_date: '2026-03-14 16:52'
labels:
  - frontend
  - onboarding
  - ux
  - agent
dependencies: []
references:
  - packages/web-ui/src/views/systems_list.rs
  - packages/web-ui/src/components/onboarding/coach_panel.rs
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem
In onboarding/wizard mode, admins can create a system record and assume Crystal Forge will automatically start tracking it. In practice, tracking only works once the target host configuration enables the Crystal Forge agent module and the agent service is actually running.

## Desired Outcome
After creating a system from onboarding flow, show a prominent reminder/notification that clearly states:
1) the Crystal Forge agent module must be enabled in the target system configuration,
2) the host must be rebuilt/applied with that config, and
3) the agent service must be running before the system will report heartbeats/metrics/deploy status.

The reminder should reduce first-time admin confusion and make the required next operational step explicit.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 After successful system creation in onboarding context, the UI shows a clear post-create reminder about enabling and running the Crystal Forge agent.
- [ ] #2 Reminder text explicitly mentions config/module enablement and agent runtime requirement on the target host.
- [ ] #3 Reminder is visible enough for first-time admins and does not block normal navigation.
- [ ] #4 Existing system creation behavior remains unchanged for non-onboarding flows.
<!-- AC:END -->

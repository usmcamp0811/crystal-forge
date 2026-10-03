---
id: TASK-472
title: Present completed POA&M detail controls as read-only
status: Backlog
assignee: []
created_date: '2026-10-03 00:22'
labels:
  - poam
  - web-ui
dependencies: []
references:
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/329'
modified_files:
  - packages/web-ui/src/components/poam/mod.rs
priority: medium
type: bug
ordinal: 493000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The server rejects every mutation of a completed POA&M with 409 poam_completed ("Completed POA&M must be reopened before mutation"), and Reopen is the only supported action. The detail drawer still renders the title, risk, assignee, target date, remediation plan, milestone and note controls as editable for any non-viewer. An operator can edit a completed plan and only learns it is rejected after saving. The old trailing metadata form had the same gap. It becomes more visible now that title, risk, assignee and target date are edited in place in the drawer header and metadata band.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 For a completed POA&M, the header title, risk, assignee, target date, remediation plan, milestone and note controls are not editable, and no Save metadata or Save plan action is offered.
- [ ] #2 Reopen remains available to authorized users, and editing becomes available again after reopening.
- [ ] #3 Viewer behavior is unchanged.
- [ ] #4 A browser assertion proves a completed POA&M exposes no editable metadata or plan control and sends no mutation request.
- [ ] #5 No server API, authorization, or persistence behavior changes.
<!-- AC:END -->

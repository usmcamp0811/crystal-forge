---
id: TASK-376
title: Fix Builders UI public key update not persisting
status: Backlog
assignee: []
created_date: '2026-06-28 02:49'
labels:
  - bug
  - ui
  - builder
milestone: Builder API hotfix
dependencies: []
priority: high
ordinal: 0
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Problem: updating a builder public key in the Crystal Forge UI and clicking Save does not persist the new key. This blocks registering API-mode builders from the UI and requires direct database updates as a workaround.

Desired Outcome: the Builders UI persists public key updates through the correct backend API and shows success/failure feedback accurately.

Non-Goals:
- Do not change builder API-mode runtime behavior.
- Do not reintroduce direct database fallback in builders.
- Do not modify deployment configuration.

Likely Impact Areas:
- Builders edit UI form/state
- Frontend API client for builder updates
- Backend builder public-key update endpoint if request handling is broken

Verification Plan:
- Use browser/network or targeted UI test to confirm saving a changed public key calls the correct endpoint.
- Confirm API/database reflects the new public key after save.
- Confirm errors are surfaced if the update fails.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Changing a builder public key in the Builders UI and clicking Save persists the new key.
- [ ] #2 The UI uses the correct backend API for public key updates or sends the expected payload to an endpoint that supports it.
- [ ] #3 The UI shows an error if the public key update fails instead of appearing to save successfully.
- [ ] #4 A regression test or targeted verification covers public key update persistence.
<!-- AC:END -->

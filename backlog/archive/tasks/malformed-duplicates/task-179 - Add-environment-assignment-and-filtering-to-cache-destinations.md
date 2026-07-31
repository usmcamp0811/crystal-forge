---
id: TASK-179
title: Add environment assignment and filtering to cache destinations
status: Backlog
assignee: []
created_date: '2026-03-10 02:57'
labels:
  - enhancement
  - cache
  - environments
dependencies: []
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Allow cache destinations to be assigned to specific environments with filtering and sorting in the UI.

**Problem**: Cache destinations are currently global with no way to:
- Assign caches to specific environments
- Filter cache list by environment
- Route builds to environment-specific caches

**Goal**: Enable per-environment cache assignment with UI filtering and sorting.

**Solution**: Add many-to-many relationship between caches and environments, update cache worker to filter by environment, add UI controls for assignment and filtering.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Environment multi-select in cache Add/Edit modals
- [ ] #2 Filter caches by environment in cache list view
- [ ] #3 Environment badges shown on cache cards
- [ ] #4 Cache worker filters destinations by build environment
- [ ] #5 Environment view shows assigned caches
- [ ] #6 Unassigned caches act as global defaults
<!-- AC:END -->

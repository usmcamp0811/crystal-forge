---
id: TASK-178
title: Add environment assignment to cache destinations
status: Backlog
assignee: []
created_date: '2026-03-10 02:56'
labels:
  - enhancement
  - cache
  - environments
  - backend
  - ui
dependencies: []
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Allow cache destinations to be assigned to specific environments, enabling per-environment cache routing.

## Problem
Currently, cache destinations are global - all builds push to all enabled caches regardless of environment. This creates issues:
- Cannot route staging builds to different caches than production
- Cannot isolate test/dev cache traffic from production
- No way to have environment-specific cache policies

## Goal
Enable per-environment cache assignment with UI support in both cache and environment views.

## Proposed Solution

### Database Schema
Add environment relationship (many-to-many via join table):
```sql
CREATE TABLE cache_destination_environments (
    cache_destination_id INTEGER REFERENCES cache_destinations(id) ON DELETE CASCADE,
    environment_id INTEGER REFERENCES environments(id) ON DELETE CASCADE,
    PRIMARY KEY (cache_destination_id, environment_id)
);
```

Alternative: Add nullable `environment_id` column if one-to-many is sufficient.

### Backend Changes
- Update CacheDestination model with environment relationship
- Add queries for environment-filtered cache destinations
- Update cache worker to filter destinations by build environment
- API endpoints for managing cache-environment assignments

### UI Changes
1. **Cache View**: Multi-select dropdown or tag selector for environments in Add/Edit modals
2. **Environment View**: Section showing assigned caches with ability to add/remove
3. Show environment badges on cache destination cards
4. Filter caches by environment

### Cache Worker Behavior
When pushing a build artifact:
- Check the build's environment
- Filter cache destinations to only those assigned to that environment (or global/unassigned caches)
- Push to filtered list

## Non-Goals
- Automatic cache selection based on flake or system metadata (separate feature)
- Cache quotas or rate limiting per environment (separate feature)
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Database migration adds environment relationship to cache destinations
- [ ] #2 Cache destination model includes environment assignment
- [ ] #3 API endpoints support assigning/unassigning environments to caches
- [ ] #4 Cache view Add/Edit modals have environment selector
- [ ] #5 Environment view shows assigned caches
- [ ] #6 Cache destination cards display environment badges
- [ ] #7 Cache worker filters destinations by build environment
- [ ] #8 Unassigned caches work as global defaults (all environments)
- [ ] #9 Tests verify environment filtering works correctly
<!-- AC:END -->

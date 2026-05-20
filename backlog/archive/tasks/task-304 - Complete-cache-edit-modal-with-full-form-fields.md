---
id: TASK-304
title: Complete cache edit modal with full form fields
status: Backlog
assignee: []
created_date: '2026-05-20 17:34'
labels:
  - ui
  - ux
  - caches
  - web-ui
milestone: UI/UX Design System
dependencies: []
priority: high
ordinal: 252000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem
The table-based cache view (TASK-303) has a placeholder edit modal that needs the full form implementation migrated from CacheDestinationCard.

## Current State
- Edit modal opens when clicking on a row or gear icon
- Shows cache name and basic info
- Has Cancel/Save button structure
- Missing: All form fields for editing cache configuration

## Goal
Migrate the complete edit form from CacheDestinationCard (~600 lines) into the centralized edit modal:
- All cache type-specific fields (Nix, S3, Attic, Http)
- Validation with field-level errors
- Environment assignment multi-select
- Real API update call
- Pre-population of existing values
- Field visibility based on cache type

## Files
- packages/web-ui/src/views/caches.rs (lines ~1153-1200 for edit modal)
- Reference: CacheDestinationCard edit modal (lines ~1269-1800)

## Acceptance Criteria
- Edit modal has all fields from CacheDestinationCard
- Fields pre-populate with current cache values
- Validation works (same rules as add modal)
- Save button calls client::update_cache_destination
- Environment assignments save via API
- Field errors display inline
- Cancel closes without saving
- Form state resets between edits
<!-- SECTION:DESCRIPTION:END -->

---
id: TASK-311
title: Refactor Builders view to pixel-perfect JSX parity
status: Done
assignee: []
created_date: '2026-05-24 01:24'
updated_date: '2026-05-26 03:34'
labels:
  - ui
  - ux
  - builders
  - web-ui
  - design-system
  - pixel-perfect
milestone: UI/UX Design System
dependencies: []
priority: high
ordinal: 250
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

The current Builders view does NOT match the reference JSX design at `/home/mcamp/code/crystal-forge/CrystalForgelatest/components/BuildersView.jsx`. The implementation is incomplete, uses wrong patterns, and lacks critical functionality.

## Goal

Implement a **PIXEL-FOR-PIXEL IDENTICAL** port of BuildersView.jsx into Dioxus. This is NOT a "close enough" task. This is NOT a "mock it out" task. This is a FULL IMPLEMENTATION with REAL BACKEND INTEGRATION.

## Design Source (AUTHORITATIVE)

- JSX Reference: `/home/mcamp/code/crystal-forge/CrystalForgelatest/components/BuildersView.jsx`
- Mock Data: `/home/mcamp/code/crystal-forge/CrystalForgelatest/data-builds.js` (BUILD_WORKERS, lines 51-57)
- Environments: `/home/mcamp/code/crystal-forge/CrystalForgelatest/data.js` (ENVIRONMENTS, lines 3-9)

## CRITICAL CONSTRAINTS (MANDATORY - NO EXCEPTIONS)

1. **PIXEL-PERFECT MATCHING REQUIRED**
   - Every spacing value MUST match JSX exactly
   - Every color MUST match JSX exactly
   - Every font size MUST match JSX exactly
   - Every class name MUST match JSX semantic classes where used

2. **REAL BACKEND INTEGRATION REQUIRED**
   - NO mocked data in the component
   - ALL data MUST come from real API endpoints
   - ALL actions (add/edit/delete) MUST call real backend APIs
   - Backend endpoints MUST be implemented if they don't exist

3. **COMPLETE FUNCTIONALITY REQUIRED**
   - ALL filters MUST work (search, status, architecture, view mode)
   - ALL modals MUST work (add builder, edit builder, delete confirmation)
   - ALL form fields MUST work and persist to database

4. **NO HALF-MEASURES ALLOWED**
   - If you run low on context, STOP and report progress
   - If something is hard, implement it anyway
   - If backend doesn't support something, ADD backend support
   - NO shortcuts. NO "good enough". NO "mostly done"

## Risk Level

HIGH: Complex full-stack feature requiring pixel-perfect UI, complete backend integration, and comprehensive state management. Cutting corners WILL result in task rejection.

## Architectural Constraints

- NO business logic in view components
- State management separated from presentation
- API client properly handles errors
- DTOs match backend models exactly
- No unwrap() in production code paths
- All user inputs validated on frontend AND backend
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 EVERY checklist item in implementation plan is complete and verified
- [ ] #2 Visual appearance is PIXEL-IDENTICAL to JSX reference (with screenshots proving it)
- [ ] #3 ALL functionality works with REAL backend data (NO mocked data in component)
- [ ] #4 ALL CRUD operations work correctly and persist to database
- [ ] #5 ALL filters work correctly and can be combined
- [ ] #6 View mode toggle (cards/table) works correctly
- [ ] #7 Add/edit/delete modals work with full validation and API integration
- [ ] #8 Environment badges display correct colors for all environment types
- [ ] #9 Builder status chips display correct colors and labels for all statuses
- [ ] #10 Progress bars calculate correctly with conditional colors
- [ ] #11 Icon sizes match JSX reference exactly (no Unicode fallbacks)
- [ ] #12 Backend API endpoints exist and return correct data structure
- [ ] #13 Database migrations applied if schema changes were needed
- [ ] #14 RBAC checks implemented (admin-only for add/edit/delete)
- [ ] #15 Error handling implemented (validation errors, API errors, network errors)
- [ ] #16 `nix develop -c cargo check` (web-ui) passes
- [ ] #17 `nix build .#checks.x86_64-linux.web-ui` passes with screenshot evidence
- [ ] #18 NO shortcuts taken, NO features mocked out, NO compromises
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
## Execution Start: 2026-05-24 01:26

Commencing pixel-perfect JSX port of Builders view.
Executing phases sequentially - no skipping allowed.

## Worktree Created

LOCK: AI agent on gray in /home/mcamp/code/crystal-forge/TASK-311-refactor-builders-view
Branch: TASK-311-refactor-builders-view
Base: dev (a8eac782)

## Phase 1 Complete (2026-05-24 01:40)

✅ All backend model fields verified/added
✅ Migration 0124 created and applied
✅ Builder model updated: host, arch, enabled, draining status
✅ All DTOs updated: CreateBuilderRequest, UpdateBuilderRequest
✅ Architecture validation added to handlers
✅ All database queries updated
✅ All test fixtures updated (10+ instances)
✅ SQLX offline metadata regenerated
✅ Compilation verified: `cargo check` passes
✅ Committed: 8c2088f3

Backend data structure ready for UI implementation.

## MR Created

Merge Request: https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/264

Created: 2026-05-24
<!-- SECTION:NOTES:END -->

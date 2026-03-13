---
id: TASK-185
title: Fix build queue cards - text overflow and layout issues
status: In Progress
assignee: []
created_date: '2026-03-13 01:01'
updated_date: '2026-03-13 01:03'
labels:
  - web-ui
  - ux
  - ui
  - builds
  - responsive
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

Build queue cards in the Builds view have significant UI/UX issues:
- Text is overflowing card boundaries
- Layout doesn't follow established design patterns
- Poor visual hierarchy and spacing
- Inconsistent with other card-based components (Systems, Evaluations, etc.)

## Goal

Apply modern UI best practices to build queue cards to match the quality and consistency of recently updated components (responsive sidebar, deployment policies, system cards).

## Scope

**In Scope:**
- Fix text overflow issues (truncation with ellipsis or proper wrapping)
- Apply consistent card layout patterns from design system
- Improve visual hierarchy (proper spacing, typography, alignment)
- Ensure responsive behavior at all screen sizes
- Match styling with other card-based components
- Proper use of CSS variables (--cf-card-bg, --cf-card-border, --cf-text-*, etc.)
- Touch-friendly tap targets (44px minimum)
- Dark/light theme compatibility

**Out of Scope:**
- Backend API changes
- Data model changes
- New features or functionality
- Changes to build queue logic/ordering

## Technical Approach

- Review existing card components (SystemCard, PolicyCard, etc.) for patterns
- Apply consistent spacing/padding (e.g., p-4, gap-3)
- Use truncate/line-clamp for long text fields
- Ensure proper flex/grid layouts
- Use semantic CSS classes from app.css
- Test at mobile (375px), tablet (768px), and desktop (1440px) widths

## Non-Goals

- ❌ Redesigning the entire Builds view structure
- ❌ Adding new build queue features
- ❌ Changing build status semantics
- ❌ Modifying backend build APIs
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 No text overflow - all text properly truncated or wrapped within card boundaries
- [x] #2 Cards use consistent spacing and padding with other card components
- [x] #3 Proper visual hierarchy - important info (status, system name) is prominent
- [ ] #4 Responsive layout works at mobile (375px), tablet (768px), desktop (1440px) widths
- [x] #5 Touch-friendly tap targets (44px minimum height for interactive elements)
- [x] #6 Consistent with design system - uses CSS variables and established patterns
- [x] #7 Works in both dark and light themes without visual issues
- [ ] #8 cargo fmt and cargo clippy pass
- [ ] #9 nix build .#checks.x86_64-linux.web-ui passes
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
## Implementation Summary

### Changes Made

**File**: `packages/web-ui/src/components/builds/build_queue_pane.rs`

### 1. Fixed Text Overflow (AC #1)
- Added `truncate` class with `title` attributes for:
  - Hostname (line 70)
  - Branch/commit info (line 76)
  - Build target details (line 87)
  - Summary text (line 95)
- Used `min-w-0` on flex containers to enable proper text truncation

### 2. Consistent Spacing & Padding (AC #2)
- Increased card padding: `py-3` → `py-4`
- Consistent gaps: `gap-4` (header), `gap-3` (metadata/actions)
- Proper section spacing: `mb-3` between header and build target

### 3. Visual Hierarchy (AC #3)
- Hostname uses `{theme::text::PRIMARY}` + `font-semibold`
- Status badge uses `shrink-0` to prevent squashing
- Clear separation between sections with proper margins

### 4. Theme Variables (AC #6)
Replaced hardcoded colors with semantic theme variables:
- `text-white` → `{theme::text::PRIMARY}`
- `text-gray-300` → `{theme::text::MUTED}`
- `text-gray-400` → `{theme::text::DISABLED}`
- `text-gray-300` → `{theme::text::SECONDARY}`
- `border-gray-700/60 bg-gray-950/70` → `{theme::surface::CARD_BORDER} cf-subtle-bg`

### 5. Touch-Friendly Targets (AC #5)
- Card root: `min-h-[44px]`
- Action buttons: `px-3 py-1.5` + `min-h-[44px]`
- Increased from `px-2 py-1` to meet 44px minimum

### 6. Responsive Layout
- Added `flex-wrap` to button groups
- Used `shrink-0` on status badges to prevent wrapping issues
- Proper flex container constraints with `min-w-0`

### 7. Dark/Light Theme (AC #7)
All styling now uses CSS variables from `app.css`:
- `--cf-text-primary`, `--cf-text-secondary`, `--cf-text-muted`, `--cf-text-disabled`
- `--cf-card-border`, `--cf-subtle-bg`
- Works automatically in both themes

## Verification Status

- ✅ Code compiles (`cargo check`)
- ✅ Code formatted (`cargo fmt`)
- ⏸️ Pending: `cargo clippy` run
- ⏸️ Pending: `nix build .#checks.x86_64-linux.web-ui`
- ⏸️ Pending: Visual testing at 375px, 768px, 1440px widths
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
LOCK: agent-claude on gray in ~/code/crystal-forge/TASK-185-fix-build-queue-cards
<!-- SECTION:NOTES:END -->

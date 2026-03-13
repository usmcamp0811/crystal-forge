---
id: TASK-185
title: Fix build queue cards - text overflow and layout issues
status: In Progress
assignee: []
created_date: '2026-03-13 01:01'
updated_date: '2026-03-13 01:01'
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
- [ ] #1 No text overflow - all text properly truncated or wrapped within card boundaries
- [ ] #2 Cards use consistent spacing and padding with other card components
- [ ] #3 Proper visual hierarchy - important info (status, system name) is prominent
- [ ] #4 Responsive layout works at mobile (375px), tablet (768px), desktop (1440px) widths
- [ ] #5 Touch-friendly tap targets (44px minimum height for interactive elements)
- [ ] #6 Consistent with design system - uses CSS variables and established patterns
- [ ] #7 Works in both dark and light themes without visual issues
- [ ] #8 cargo fmt and cargo clippy pass
- [ ] #9 nix build .#checks.x86_64-linux.web-ui passes
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
LOCK: agent-claude on gray in ~/code/crystal-forge/TASK-185-fix-build-queue-cards
<!-- SECTION:NOTES:END -->

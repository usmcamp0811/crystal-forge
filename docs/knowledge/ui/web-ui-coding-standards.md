---
type: UI Design
title: "Web UI Coding Standards"
description: "Defines styling rules, the inline style policy, the theme token policy, and the review checklist for packages/web-ui."
tags:
  - crystal-forge
  - web-ui
  - styling
  - theme
  - standards
implementation_status: implemented
verified:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-04T08:26:03-05:00
sources:
  - id: origin
    resource: "Crystal Forge repository file docs/web-ui-coding-standards.md at commit 3b23d36f"
    title: "Web UI Coding Standards"
  - id: code-1
    resource: "Crystal Forge repository file packages/web-ui/assets/app.css at commit 3b23d36f"
    title: "app.css"
  - id: code-2
    resource: "Crystal Forge repository file packages/web-ui/src/components/layout/sidebar.rs at commit 3b23d36f"
    title: "sidebar.rs"
---
# Web UI Coding Standards

> **See Also:**
> - **[Frontend Component Isolation Standards](component-isolation-standards.md)** - Component taxonomy, isolation workflow, and state coverage requirements
> - **[UI/UX Design System](design-system-overview.md)** - Design philosophy, component patterns, accessibility, and anti-patterns

## Scope

This document defines styling standards for `packages/web-ui` (Dioxus frontend).

## Styling Rules

1. Use shared classes and theme tokens for repeated static styles.
2. Keep inline `style` only for runtime-calculated values that cannot be represented as static classes.
3. Introduce semantic CSS classes in `packages/web-ui/assets/app.css` instead of duplicating literal color/style strings in components.
4. Prefer CSS variables for color and surface primitives to support theme variants.
5. Keep UI behavior unchanged when extracting styles (no incidental redesign).

## Inline Style Policy

Inline `style` is allowed only when values are runtime-dependent, for example:

- Timeline node positions/sizes computed from data
- Grid coordinates computed from user layout
- Per-item colors derived from API/runtime values

Inline `style` is not allowed for repeated static values, including:

- Modal overlay geometry and blur
- Repeated badge/chip background and border colors
- Repeated action link colors
- Reused gradient backgrounds

## Theme Token Policy

`packages/web-ui/assets/app.css` is the source of truth for theme tokens.

- Default/dark theme is defined in `:root, :root[data-theme="dark"]`
- Light theme overrides are defined in `:root[data-theme="light"]`
- Additional custom themes should be added as `:root[data-theme="<name>"]`

Components should consume semantic classes backed by tokens (for example, `cf-chip-info`, `cf-modal-overlay`) instead of hardcoded style literals.

## Review Checklist (UI Changes)

- No new repeated static inline style literals were introduced.
- New static visual primitives were added as tokens/classes in `app.css`.
- Any remaining inline styles are documented in task notes as runtime-calculated.
- Dark and light token mappings still render acceptably.

## Migration verification notes

- Claim: Theme tokens live in app.css (dark default in `:root, :root[data-theme="dark"]`, light in `:root[data-theme="light"]`); semantic classes such as `cf-chip-info`, `cf-modal-overlay`.
  Finding: Confirmed. Both classes exist. The inline-style policy and review checklist are process rules; existing code still contains static inline styles (for example the sidebar `border-right` style), so the policy is a review rule, not an enforced invariant.
  Evidence: assets/app.css; components/layout/sidebar.rs
  Case: none

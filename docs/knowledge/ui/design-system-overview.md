---
type: UI Design
title: "Crystal Forge UI/UX Design System"
description: "Defines the authoritative Crystal Forge web UI design philosophy, technology stack, and source-of-truth hierarchy, and indexes the six design-system concepts."
tags:
  - crystal-forge
  - web-ui
  - design-system
  - dioxus
implementation_status: implemented
generated:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-03T22:54:52-05:00
verified:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-04T08:26:03-05:00
sources:
  - id: origin
    resource: "Crystal Forge repository file docs/ui-ux-design-system.md at commit 3b23d36f"
    title: "Crystal Forge UI/UX Design System"
  - id: code-1
    resource: "Crystal Forge repository file packages/web-ui/assets/app.css at commit 3b23d36f"
    title: "app.css"
  - id: code-2
    resource: "Crystal Forge repository file packages/web-ui/src/theme.rs at commit 3b23d36f"
    title: "theme.rs"
  - id: code-3
    resource: "Crystal Forge repository file packages/web-ui/src/routes.rs at commit 3b23d36f"
    title: "routes.rs"
  - id: code-4
    resource: "Crystal Forge repository file packages/web-ui/tailwind.css at commit 3b23d36f"
    title: "tailwind.css"
  - id: code-5
    resource: "Crystal Forge repository file packages/web-ui/Cargo.toml at commit 3b23d36f"
    title: "Cargo.toml"
---

# Crystal Forge UI/UX Design System

This document defines the authoritative UI/UX standards for Crystal Forge. All agents implementing UI changes MUST follow these guidelines.

**Document Version:** 1.0  
**Last Updated:** 2026-03-10  
**Applies to:** `packages/web-ui` (Dioxus frontend)

## Table of Contents

> **Status:** This document was split into six concepts. Each entry below links to the concept that now holds the section.

1. [Design Philosophy](#design-philosophy)
2. [Technology Stack](#technology-stack)
3. [Theming System](design-system-theming-and-color.md#theming-system)
4. [Color System](design-system-theming-and-color.md#color-system)
5. [Typography](design-system-typography-and-layout.md#typography)
6. [Spacing & Layout](design-system-typography-and-layout.md#spacing--layout)
7. [Component Patterns](design-system-components-and-interaction.md#component-patterns)
8. [Interaction Patterns](design-system-components-and-interaction.md#interaction-patterns)
9. [Accessibility Requirements](design-system-accessibility-responsive-motion.md#accessibility-requirements)
10. [Responsive Design](design-system-accessibility-responsive-motion.md#responsive-design)
11. [Animation & Motion](design-system-accessibility-responsive-motion.md#animation--motion)
12. [Naming Conventions](design-system-conventions-and-anti-patterns.md#naming-conventions)
13. [Anti-Patterns](design-system-conventions-and-anti-patterns.md#anti-patterns)
14. [Decision Framework](design-system-conventions-and-anti-patterns.md#decision-framework)

## Design Philosophy

Crystal Forge follows a **Professional/Enterprise** design philosophy optimized for infrastructure operations teams.

### Core Principles

1. **Clarity over decoration** - Every visual element must serve a functional purpose
2. **Data density with readability** - Show relevant information without overwhelming
3. **Consistent feedback** - Users always know what's happening and what they can do
4. **Keyboard-first** - All actions accessible without a mouse
5. **Dark-first** - Optimized for dark theme; light theme is secondary but fully supported

### Design Goals

| Goal | Implementation |
|------|----------------|
| Scannable dashboards | Status colors, consistent badge placement, clear hierarchy |
| Quick actions | Prominent CTAs, predictable button locations |
| Error visibility | Red indicators, toast notifications, inline feedback |
| Information density | Compact cards, tabular data, collapsible sections |

## Technology Stack

| Layer | Technology | Purpose |
|-------|------------|---------|
| Framework | Dioxus 0.7 (Rust/WASM) | Reactive UI components |
| Styling | Tailwind CSS v4 | Utility-first CSS |
| Theming | CSS Custom Properties | Dark/light theme switching |
| Icons | Inline SVG (Heroicons style) | Stroke-based, consistent sizing |
| State | Dioxus Signals + Context | Reactive state management |
| Real-time | WebSocket hooks | Live logs, build status |

### Source of Truth Hierarchy

1. **`packages/web-ui/assets/app.css`** - CSS variables and semantic classes
2. **`packages/web-ui/src/theme.rs`** - Rust constants mapping to CSS classes
3. **This document** - Design rationale and patterns
4. **`/style-guide` route** - Live visual reference

## Visual Reference

For live examples of all components and tokens:

1. Run the development server
2. Navigate to `/style-guide`
3. Toggle light/dark theme to verify both modes

Screenshots are available in `docs/screenshots/` for offline reference.

## Changelog

| Version | Date | Changes |
|---------|------|---------|
| 1.0 | 2026-03-10 | Initial design system document |

## Related concepts

- [Crystal Forge UI/UX Design System (overview)](design-system-overview.md)
- [Design System: Theming and Color System](design-system-theming-and-color.md)
- [Design System: Typography, Spacing and Layout](design-system-typography-and-layout.md)
- [Design System: Component and Interaction Patterns](design-system-components-and-interaction.md)
- [Design System: Accessibility, Responsive Design and Motion](design-system-accessibility-responsive-motion.md)
- [Design System: Naming Conventions, Anti-Patterns and Decision Framework](design-system-conventions-and-anti-patterns.md)
- [Web UI Coding Standards](web-ui-coding-standards.md)
- [Frontend Component Isolation Standards](component-isolation-standards.md)

## Migration verification notes

- Claim: Technology stack (Dioxus 0.7, Tailwind v4, CSS custom properties, WebSocket hooks) and source-of-truth files exist.
  Finding: Confirmed: dioxus 0.7 with web+router features, tailwind.css banner v4.1.18, tokens in app.css, constants in src/theme.rs, /style-guide route registered, hooks in src/hooks/websocket.rs. Principles and goals are normative statements, not code-checkable.
  Evidence: packages/web-ui/Cargo.toml; tailwind.css; assets/app.css; src/theme.rs; src/routes.rs (StyleGuideView)
  Case: none (documentation accurate)

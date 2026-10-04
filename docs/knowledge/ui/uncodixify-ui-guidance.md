---
type: UI Design
title: "Uncodixify UI guidance"
description: "Summarizes the Uncodixify guide, which tells agents to build plain, human-designed UI and lists banned default AI UI patterns and palette inspirations."
tags:
  - crystal-forge
  - web-ui
  - agent-guidance
  - anti-patterns
implementation_status: implemented
generated:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-03T22:54:52-05:00
sources:
  - id: s1
    resource: "Crystal Forge repository file backlog/docs/doc-4%20-%20Uncodixify.md at commit 3b23d36f"
    title: "Uncodixify"
---
# Uncodixify UI guidance

This pointer concept describes Backlog document `doc-4`, [Uncodixify](../../../backlog/docs/) (file `doc-4 - Uncodixify.md`). Backlog.md manages the document by ID, so it stays at its path. The retained file is the authoritative text.

## What it specifies

The document is a prompt-style guide for agents that build UI. It lists the default "AI aesthetic" to avoid and the plain alternative to build. Its sections:

- **Keep It Normal.** A per-element standard for sidebars, headers, sections, navigation, buttons, cards, forms, inputs, modals, dropdowns, tables, lists, tabs, badges, avatars, icons, typography, spacing, borders, shadows, transitions, layouts, grids, containers, panels, toolbars, footers, and breadcrumbs. Examples: a fixed sidebar 240 to 260 pixels wide, button and card radii of 8 to 12 pixels at most, shadows no larger than a subtle 8-pixel blur, transitions of 100 to 200 milliseconds.
- **Hard No.** A list of banned patterns, among them oversized rounded corners, glassmorphism, decorative gradients, hero sections in internal dashboards, eyebrow labels, `<small>` headers, metric-card grids as the first instinct, decorative copy, and "control room" styling.
- **Specifically Banned.** A list of concrete mistakes from an earlier generated UI.
- **Rule and colors.** The color priority order is: use the project's existing palette, else take inspiration from the listed dark and light palettes, never invent random combinations. The document ends with ten dark and ten light palette tables.
- **Replicate designer components.** It tells the agent to replicate Figma or designer-made components rather than invent new ones.

## Status

> **Status:** implemented as guidance, not as code. The document constrains agent behavior and has no code counterpart. Some rules differ from the Crystal Forge design system, for example the sidebar width and the use of nav badges: the design system defines a fixed 256-pixel sidebar and the product has functional sidebar badges. The design system and the owner's Claude design take precedence for Crystal Forge UI. This was not compared line by line with the code.

## Related concepts

- [Crystal Forge UI/UX Design System](design-system-overview.md)
- [Design System: Naming Conventions, Anti-Patterns and Decision Framework](design-system-conventions-and-anti-patterns.md)
- [Web UI Coding Standards](web-ui-coding-standards.md)

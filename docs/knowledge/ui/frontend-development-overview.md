---
type: UI Design
title: "Frontend Development"
description: "Summarizes how the Dioxus web UI is developed with isolation-driven components and links the frontend standards documents; open it to find the right UI standards document."
tags:
  - crystal-forge
  - ui
  - frontend
  - dioxus
implementation_status: implemented
generated:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-03T22:57:15-05:00
verified:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-04T08:26:03-05:00
sources:
  - id: origin
    resource: "Crystal Forge repository file docs/architecture.md at commit 3b23d36f"
    title: "ADR-000: Crystal Forge Architecture Overview"
  - id: code-1
    resource: "Crystal Forge repository file packages/web-ui/src/views/style_guide.rs at commit 3b23d36f"
    title: "style_guide.rs"
---

# Frontend Development

Crystal Forge uses Dioxus for the web UI with isolation-driven component development practices.

**Key Documentation:**
- **[Frontend Component Isolation Standards](component-isolation-standards.md)** - Component taxonomy, isolation workflow, state coverage, and contribution guidelines
- **[Frontend Views Specification](frontend-navigation-and-shared-patterns.md)** - View-level architecture and data flows
- **[Web UI Coding Standards](./web-ui-coding-standards.md)** - Styling and theme token policies
- **[UI/UX Design System](design-system-overview.md)** - Design philosophy and patterns

## Related concepts

- [System overview](../overview/system-overview.md) - where the web UI sits in the architecture
- [ADR-000 Architecture overview](../decisions/adr-000-architecture-overview.md) - the originating decision record

## Migration verification notes

- Claim: Links four frontend standards documents and states isolation-driven component development.
  Finding: All four linked concepts exist; the isolation surface exists as `views/style_guide.rs` (`Component Isolation Surface`) and `src/showcase/`.
  Evidence: views/style_guide.rs; src/showcase
  Case: none

---
id: TASK-8
title: UI Development - Foundation and Architecture
status: To Do
assignee: []
created_date: '2026-02-05 14:14'
labels:
  - ui
  - foundation
  - dioxus
dependencies: []
priority: high
milestone: m-3
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Set up the foundational architecture for Crystal Forge UI using Dioxus framework (web target only). Validate technology choices, establish project structure, and create core infrastructure.

Architecture decisions (Feb 2026):
- **Framework**: Dioxus (web target only)
- **TUI**: Deferred to future milestone (TASK-8.2 cancelled; Ratatui considered for later)
- **Styling**: Tailwind CSS with dark theme defaults
- **Package location**: Separate crate at packages/web-ui/ (server deps incompatible with wasm32)
- **Production serving**: Embedded in axum server binary
- **Data models**: Separate API DTOs decoupled from DB models
- **Charts**: Stretch goal (simple counters/badges for v1)
- **WebSocket**: Deferred (polling for v1)
- **Auth**: Deferred to next milestone

New subtasks added: TASK-8.9 (Nix dev shell tooling), TASK-8.10 (Tailwind build pipeline), TASK-8.11 (Embed assets in axum)
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Dioxus web app builds and runs successfully via trunk serve
- [ ] #2 API client (HTTP + Mock) can communicate with backend
- [ ] #3 Tailwind dark theme design system implemented
- [ ] #4 Dioxus signals state management architecture in place
- [ ] #5 Built UI assets embedded in axum server for production
- [ ] #6 Nix dev shell includes trunk, wasm-bindgen-cli, wasm-opt
<!-- AC:END -->

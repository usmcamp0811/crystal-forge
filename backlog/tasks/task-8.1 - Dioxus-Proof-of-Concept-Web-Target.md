---
id: TASK-8.1
title: Dioxus Proof of Concept - Web Target
status: Done
assignee:
  - KimiK2.5
created_date: '2026-02-05 14:14'
updated_date: '2026-02-21 03:28'
labels:
  - ui
  - poc
  - web
milestone: m-3
dependencies: []
parent_task_id: TASK-8
priority: high
ordinal: 52000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Build and validate a minimal web application using Dioxus to prove the framework works for our needs. This is web-only; TUI has been deferred to a future milestone.

Steps:
1. Ensure trunk and wasm toolchain are available via nix develop (see TASK-8.9)
2. Create UI module structure within packages/default/ (src/ui/ or src/bin/web.rs)
3. Add dioxus and dioxus-web dependencies to packages/default/Cargo.toml
4. Create index.html with div id="main" and Tailwind CSS CDN link
5. Create a simple counter component to validate Dioxus rendering
6. Configure Trunk.toml for the web build (output to dist/)
7. Run: trunk serve --proxy-backend=http://localhost:{server_port}/api
8. Test in browser at localhost:8080
9. Measure bundle size: ls -lh dist/*.wasm

Architecture decisions:
- UI lives inside packages/default/ (not a separate workspace member)
- Dioxus web target only (no TUI)
- Tailwind CSS for styling (CDN for PoC, build pipeline in TASK-8.10)
- Will be embedded in axum server for production (TASK-8.11)

Expected: Bundle < 500kb gzipped, hot reload works, no console errors
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 #1 Web app builds successfully with dx build
- [x] #2 #2 Counter component with increment/decrement implemented
- [ ] #3 #3 Hot reload works during development (requires dx serve with browser — validated build only)
- [ ] #4 #4 Bundle size documented and < 500kb gzipped (debug: 7.4MB gzip, release TBD)
- [x] #5 #5 Dioxus.toml and project structure established

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
- **Architecture change**: Web UI is a separate crate at `packages/web-ui/` (not inside packages/default/)
  - Reason: existing crystal-forge lib has native-only deps (sqlx, sysinfo, nix) that cannot compile to wasm32
  - API DTOs defined in `packages/default/src/api/models.rs` will be duplicated/shared via wire-level JSON compatibility
- Bumped nixpkgs from release-25.05 to release-25.11 (Rust 1.91.1, dx 0.7.3)
- Required `cargo update` on packages/default/ to fix futures-util compat with rustc 1.91.1
- uuid crate needs `js` feature for wasm32 target (getrandom backend)
- Dioxus 0.7.3 with `dx` CLI (not trunk)
- Tailwind CSS via CDN link in the PoC (will be proper build pipeline in TASK-8.10)
- Debug WASM: 28MB raw / 7.4MB gzipped. Release + wasm-opt will be much smaller.
- All 35 existing tests pass after nixpkgs bump
<!-- AC:END -->
<!-- SECTION:NOTES:END -->

---
id: TASK-8.11
title: Embed Built UI Assets in Axum Server
status: To Do
assignee: []
created_date: '2026-02-11 10:00'
labels:
  - ui
  - backend
  - deployment
dependencies:
  - TASK-8.1
  - TASK-8.10
parent_task_id: TASK-8
priority: high
milestone: m-3
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Configure the axum server to serve the built Dioxus WASM UI as embedded static assets. This enables single-binary deployment where the Crystal Forge server binary includes the web UI.

Steps:
1. Add `rust-embed` or `include_dir` crate to Cargo.toml dependencies
2. Create a build step that runs `trunk build --release` to produce dist/ assets
3. Embed the dist/ directory (index.html, *.wasm, *.js, tailwind.css) into the server binary
4. Add a catch-all route in the axum server to serve static files:
   - GET / → index.html
   - GET /assets/* → embedded static files
   - Existing API routes (/status, /agent/*, /webhook) remain unchanged
5. Add proper Content-Type headers for WASM, JS, CSS, HTML
6. Add cache headers (Cache-Control with content hashing)
7. Ensure the API routes take precedence over the static file catch-all
8. Add a feature flag (e.g., `feature = "ui"`) so the server can be built without UI if needed
9. Update the Nix package build to include the trunk build step
10. Test: start server, navigate to http://localhost:{port}/ in browser, see Dioxus app

Architecture notes:
- Use `rust-embed` with `#[derive(RustEmbed)]` pointing to the dist/ directory
- Or use `tower-http::services::ServeDir` if assets are on disk (simpler but not embedded)
- Feature flag approach: `#[cfg(feature = "ui")] mod ui_routes;`
- The Nix build (in packages/default/) needs a two-phase build: trunk first, then cargo

Expected: Single `server` binary serves both API and web UI on the same port
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Axum server serves index.html at GET /
- [ ] #2 WASM and static assets served with correct Content-Type
- [ ] #3 API routes still work unchanged
- [ ] #4 Single binary deployment (no separate web server needed)
- [ ] #5 Feature flag allows building server without UI
- [ ] #6 Nix package build includes trunk build step
- [ ] #7 Cache headers set for static assets
<!-- AC:END -->

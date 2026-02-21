---
id: TASK-8.11
title: Embed Built UI Assets in Axum Server
status: Done
assignee:
  - KimiK2.5
created_date: '2026-02-11 10:00'
updated_date: '2026-02-21 03:28'
labels:
  - ui
  - backend
  - deployment
milestone: m-3
dependencies:
  - TASK-8.1
  - TASK-8.10
parent_task_id: TASK-8
priority: high
ordinal: 14000
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

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
LOCK: OpenCode on gray in /home/mcamp/code/crystal-forge/TASK-8.11-embed-built-ui-assets

Implemented embedded UI serving behind feature flag: added handlers/ui.rs for SPA+asset responses with content-type and cache headers; wired fallback route in server binary under embedded-ui feature; added include_dir/mime_guess optional deps; enabled Nix build with embedded-ui feature and wired CRYSTAL_FORGE_UI_DIST to web-ui package public assets; kept build.rs migration rerun plus env fallback for local builds.

Verification: nix develop -c env SQLX_OFFLINE=true cargo check --features embedded-ui (pass), nix develop -c env SQLX_OFFLINE=true cargo test --lib --features embedded-ui (pass, 87 tests), nix build .#packages.x86_64-linux.server (pass).

Verification caveats: nix develop -c cargo fmt -- --check fails due pre-existing repository formatting drift in unrelated files; nix develop -c env SQLX_OFFLINE=true cargo clippy --features embedded-ui -- -D warnings fails due existing repo-wide warnings and toolchain artifact mismatch (E0514).

Commit: 1fe0f62 (feat: embed web UI assets in server build)

MR: https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/110

Blocker fix for MR validation: default systems[].deployment_policy to manual when missing in legacy config to prevent startup parse failure during server-stack boot.

Added tests in config/system.rs for missing-field defaulting and explicit deployment policy preservation.

Verification (blocker fix): nix develop -c env SQLX_OFFLINE=true cargo test --lib config::system -- --nocapture (pass), nix develop -c env SQLX_OFFLINE=true cargo check --features embedded-ui (pass), nix build .#packages.x86_64-linux.server (pass).

Merged to dev; closed during sprint grooming.
<!-- SECTION:NOTES:END -->

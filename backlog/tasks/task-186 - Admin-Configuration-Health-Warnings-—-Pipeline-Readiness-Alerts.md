---
id: TASK-186
title: Admin Configuration Health Warnings — Pipeline Readiness Alerts
status: In Progress
assignee: []
created_date: '2026-03-13 01:16'
updated_date: '2026-03-14 17:35'
labels:
  - frontend
  - backend
  - admin
  - ux
  - onboarding
dependencies: []
references:
  - docs/eval-build-deploy-flow.md
  - docs/specs/00-system-overview.md
  - packages/default/src/handlers/api/auth_status.rs
  - packages/web-ui/src/views/dashboard.rs
  - packages/web-ui/src/components/layout/sidebar.rs
  - packages/default/src/config/mod.rs
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

When a new admin stands up Crystal Forge for the first time, they create a system and a flake and expect things to work — but they don't. There is no feedback about *why*. The deployment pipeline has multiple stages (commit detection → evaluation → policy check → build → cache push → agent deployment), and a missing configuration at any stage causes a silent failure. The admin has no way to know what's misconfigured without independently understanding the full pipeline architecture.

## Goal

Admin users receive clear, actionable warnings about configuration gaps that prevent the deployment pipeline from functioning. Warnings appear in three locations: a dashboard health widget, contextual inline warnings on entity views, and a global notification bar. Warnings are visible only to Admin users.

Feature-complete UI changes for this task must also produce screenshots tied to each affected feature surface. If existing UI-check or screenshot automation is insufficient, this task includes updating that workflow so the required screenshots can be captured deterministically and attached to the MR.

## Non-Goals

- This task does NOT implement a guided setup wizard (see TASK-187).
- This task does NOT add remediation forms inline (warnings link to existing pages; they don't embed creation forms).
- This task does NOT change the behavior of the deployment pipeline itself — it only surfaces existing failures as UI warnings.
- This task does NOT add email/webhook/external notifications — only in-app UI warnings.
- This task does NOT modify the evaluation, build, or deployment logic.
- This task does NOT add warning persistence to a database — all state is derived dynamically from existing entity counts/associations.

## Configuration Health Checks

The following pipeline readiness checks must be implemented, mapped to the eval-build-deploy flow:

### Global Level (Dashboard widget + notification bar)
1. **No Flakes configured** → "No flakes are being watched. Add a flake to begin evaluating NixOS configurations."
2. **No Environments created** → "No environments exist. Environments are required to organize systems, builders, and caches."
3. **No Builders registered** → "No builders are registered. Derivations will be evaluated but never built."
4. **No Cache Destinations configured** → "No cache destinations configured. Builds will succeed but agents won't be able to pull deployments."

### System View (contextual, per-system)
5. **System has no flake_id** → "This system is not linked to a flake. It won't be included in evaluations."
6. **System has no connected agent** → "No agent heartbeat detected. This system cannot receive deployments."

### Environment View (contextual, per-environment)
7. **Environment has no builder assigned** → "No builder is assigned to this environment. Builds for systems in this environment won't be processed."
8. **Environment has no cache destination assigned** → "No cache destination is assigned to this environment. Builds for this environment won't be deployable."

### Flakes View (contextual)
9. **Flake has evaluation errors on latest commit** → "Latest evaluation failed. Check flake configuration."

## Architectural Constraints

- **Backend**: New `GET /api/v1/admin/config-health` endpoint following the existing handler pattern. Uses `require_admin()` from `handlers/api/rbac.rs` for access control. Aggregates counts via `sqlx::query_scalar` using the same patterns as `queries/dashboard.rs`. Uses `tokio::try_join!` for parallel count queries. Returns a structured `ConfigHealthResponse` with per-check status.
- **Frontend API**: New function in `api/client.rs` using `fetch_json()` to call the health endpoint. Follow the adapter pattern (see `dashboard/adapter.rs`) with fallback support and login redirect detection.
- **Frontend components**: New reusable `AlertBanner` component in `components/notifications/` (following the `Toast` component pattern) for consistent warning rendering across views. Supports severity levels (warning/info), dismissibility, and action links.
- **Global notification bar**: Inserted in `app_shell.rs` between `DevModeBanner { Top }` and the main flex container. Conditionally rendered only for admin users (check `is_admin()` from `state/auth.rs`). Dismissible per browser session via `web_sys::window().local_storage()`. Reappears if health status changes (compare hash of issues list).
- **Dashboard widget**: New widget added to `default_widget_positions()` and `render_widget_content()` in `dashboard.rs`. Renders a summary card showing pipeline readiness with a list of unresolved issues and action links.
- **Contextual warnings**: Added to existing entity view components (`systems_list.rs`, `environments_list.rs`, `flakes_list.rs`) as `AlertBanner` instances above the entity list, rendered conditionally based on health data or entity-specific field checks.
- **Admin-only visibility**: All warning rendering gated behind `is_admin(&app_state.read().auth)` check. Non-admin users see standard empty states unchanged.
- **UI evidence**: The task must produce screenshots for each user-visible warning surface changed by this work (dashboard widget, global notification bar, systems warning state, environments warning state, flakes warning state). If existing `web-ui` check coverage cannot deterministically capture those states, update the relevant UI-check/screenshot mechanism as part of this task.
- **MR usage**: Captured screenshots must be uploaded to GitLab and referenced from the MR description for this task.
- **No new database tables or migrations required** — all health checks are derived from counts of existing entities and their associations.
- **Contextual per-entity checks** (system.flake_id, environment builder/cache assignments) can use data already returned by existing list endpoints — no additional API calls needed for those.

## Impact Areas

- `packages/default/src/handlers/api/` — new admin health endpoint handler
- `packages/default/src/bin/server.rs` — route registration
- `packages/default/src/queries/` — new count/existence queries (or reuse existing)
- `packages/web-ui/src/api/client.rs` — new API call function
- `packages/web-ui/src/components/notifications/` — new AlertBanner component
- `packages/web-ui/src/components/layout/app_shell.rs` — global notification bar insertion
- `packages/web-ui/src/views/dashboard.rs` — new health widget
- `packages/web-ui/src/views/systems_list.rs` — contextual system warnings
- `packages/web-ui/src/views/environments_list.rs` — contextual environment warnings
- `packages/web-ui/src/views/flakes_list.rs` — contextual flake warnings
- `packages/web-ui` UI-check / screenshot automation — updated if needed to capture the required evidence deterministically for MR use

## Risk Level

**Medium** — Touches multiple frontend views and adds a new API endpoint, but the changes are additive (no modification of existing logic). Risk is primarily in getting the warning conditions correct and ensuring they don't produce false positives when the pipeline is partially configured intentionally.

## Verification Plan

- **Tier 0**: `cargo fmt -- --check`, `cargo clippy -- -D warnings`, `cargo test` (targeted: health endpoint handler tests, query tests)
- **Tier 1**: Start full stack (`server-stack up`), verify:
  - Fresh instance with no entities shows all global warnings on dashboard and notification bar
  - Adding a flake removes the "no flakes" warning
  - Adding a builder removes the "no builders" warning
  - Non-admin user does NOT see any warnings
  - Notification bar dismisses on click and stays dismissed during session
  - Notification bar reappears after adding/removing entities that change health status
  - Contextual warnings appear on correct entity views
- **UI Evidence**: Capture deterministic screenshots for every affected warning surface. If current UI-check coverage cannot produce them, extend it and rerun it so screenshots can be attached to the MR.
- **Tier 2**: `nix flake check` — required since new API endpoint affects the server package
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A new `GET /api/v1/admin/config-health` endpoint exists that returns a structured JSON response with boolean/count fields for each health check (flakes, environments, builders, caches, and aggregate issue count). Returns 403 for non-admin users.
- [ ] #2 The endpoint response includes at minimum: `has_flakes`, `has_environments`, `has_builders`, `has_cache_destinations`, `total_issues` count, and a `checks` array with per-check `id`, `passed`, `message`, and `action_url`.
- [ ] #3 A reusable `AlertBanner` component exists in `components/notifications/` that accepts severity (warning/info), message text, optional action link, and optional dismiss handler.
- [ ] #4 The Dashboard view includes a Configuration Health widget that displays when the admin is logged in AND at least one health check fails. The widget lists all failing checks with actionable links to the relevant configuration page.
- [ ] #5 A global notification bar appears at the top of the layout (in `app_shell.rs`) for admin users when health checks fail. It summarizes the count of configuration issues (e.g., '4 configuration issues detected') and links to the dashboard health widget.
- [ ] #6 The global notification bar is dismissible per browser session (using localStorage). It reappears if the set of failing checks changes.
- [ ] #7 The Systems list view shows an inline warning banner for any system where `flake_id` is null, stating the system won't be evaluated.
- [ ] #8 The Systems list view shows an inline warning banner for any system with no recent agent heartbeat, stating the system cannot receive deployments.
- [ ] #9 The Environments list view shows an inline warning banner for any environment with no builder assigned, stating builds won't process for that environment.
- [ ] #10 The Environments list view shows an inline warning banner for any environment with no cache destination assigned, stating builds won't be deployable for that environment.
- [ ] #11 The Flakes list view shows an inline warning banner when the latest commit for a flake has an evaluation error.
- [ ] #12 All warning UI (dashboard widget, notification bar, contextual banners) is gated behind `is_admin()` — non-admin users (Operator, Viewer) see no warnings and experience no behavior change.
- [ ] #13 When all health checks pass (fully configured instance), no warnings appear anywhere — the dashboard widget is hidden, the notification bar is hidden, and no contextual banners render.
- [ ] #14 Unit tests exist for the config-health endpoint handler covering: all checks failing (empty instance), all checks passing (fully configured), partial configuration, and 403 for non-admin users.
- [ ] #15 The health endpoint queries run efficiently using COUNT queries (not loading full entity lists) and use `tokio::try_join!` for parallel execution.
- [ ] #16 Feature-specific screenshots exist for each user-visible UI surface changed by this task: dashboard Pipeline Readiness widget, global notification bar, systems warning state, environments warning state, and flakes warning state.
- [ ] #17 If current `web-ui` checks or screenshot tooling cannot capture the required states deterministically, the task updates that workflow so the screenshots can be generated reproducibly.
- [ ] #18 The Merge Request for this task includes the captured UI screenshots as GitLab uploads referenced in the MR description.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
## Sprint-Ready Review Confirmed (2026-03-13)

All fields verified against codebase before promotion to To Do.

### Key pattern anchors found:
- **RBAC guard**: `require_admin(pool, &headers)` from `handlers/api/rbac.rs` — use exact pattern from `dashboard.rs` handler
- **Parallel queries**: `tokio::try_join!` with `sqlx::query_as::<_, (i64,)>("SELECT COUNT(*) FROM ...")` — see `queries/dashboard.rs`
- **Route registration**: follows `.route("/api/v1/...", get(handler::fn))` pattern in `bin/server.rs`
- **Model location**: new `ConfigHealthResponse` and `ConfigHealthCheck` structs go in `api/models.rs`
- **API client**: new `fetch_config_health()` in `api/client.rs` using `fetch_json()` — follow `fetch_dashboard()` pattern
- **Alert component**: `components/notifications/` already exists (has `toast.rs`) — add `alert_banner.rs` there; follow `Toast` component signature style
- **App shell injection**: insert global notification bar after `DevModeBanner { placement: BannerPlacement::Top }` line in `app_shell.rs`
- **Admin gate in frontend**: `state::auth::is_admin(&app_state.read().auth)` — confirmed present and tested
- **Dashboard widget hook**: `default_widget_positions()` + `render_widget_content()` in `views/dashboard.rs` — add new widget ID there
- **`evaluation_error_message`**: exists on commits table (`queries/commits.rs`) — health check query can COUNT flakes where latest commit has non-null `evaluation_error_message`
- **Environment builder/cache**: `EnvironmentSummary` does NOT expose builder/cache counts; health endpoint derives these via independent COUNT queries on association tables — no model change needed for the endpoint itself

### Clarification on AC #11 (flake eval errors):
Query pattern: `COUNT(*) FROM flakes f JOIN commits c ON c.flake_id = f.id WHERE c.evaluation_error_message IS NOT NULL AND c.created_at = (SELECT MAX(...))` or equivalent latest-commit subquery.

### No dependencies unmet.
No schema migrations required. All checks derived from existing tables/counts.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
MR: https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/163
Verification: cargo check (web-ui) ✅, rustfmt --check ✅, handler unit tests included. Awaiting merge into dev.

Requirement update: UI changes for this task now require feature-specific screenshots. If needed, extend the `web-ui` check/screenshot workflow to capture them deterministically, and use those screenshots in the MR.

LOCK: OpenCode on reckless in ~/code/crystal-forge/TASK-186-admin-config-health

Implemented remaining systems warning gap: systems list data now carries `flake_id` through backend and web-ui summaries, and the systems view shows an admin warning when one or more systems are not linked to a flake.

Expanded `checks/web-ui/tests/integration-test.js` to capture feature-specific screenshots for config health surfaces: `06b-config-health-bar.png`, `06c-config-health-widget.png`, `12b-systems-config-warning.png`, `13b-flakes-config-warning.png`, and `14b-environments-config-warning.png`. Artifacts are present under `result/screenshots/` after `nix build .#checks.x86_64-linux.web-ui`.

Verification run: `cargo check` passed for `packages/default` and `packages/web-ui`; targeted web-ui helper tests passed; touched Rust files pass `rustfmt --check`; `nix build .#checks.x86_64-linux.web-ui` passed and produced screenshots; `nix flake check` completed successfully.

Verification caveat: `cargo clippy -- -D warnings` did not complete cleanly in this worktree because the repo currently has unrelated/pre-existing warning debt plus target cache rustc-version mismatch errors under clippy. MR screenshot upload/description update is still pending.
<!-- SECTION:NOTES:END -->

---
id: TASK-187
title: First-Time Admin Setup Wizard — Guided Onboarding Flow
status: In Progress
assignee: []
created_date: '2026-03-13 01:16'
updated_date: '2026-03-13 12:58'
labels:
  - frontend
  - backend
  - admin
  - ux
  - onboarding
dependencies: []
references:
  - docs/eval-build-deploy-flow.md
  - packages/web-ui/src/views/register.rs
  - packages/default/src/handlers/api/auth_status.rs
  - modules/nixos/crystal-forge/default.nix
  - docs/specs/00-system-overview.md
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

After a new admin registers and logs into Crystal Forge for the first time, they land on an empty dashboard with no guidance. They must independently know the correct setup sequence to get a working deployment pipeline: create environments → add flakes → register builders → configure cache → add systems → deploy agents. Missing any step results in silent pipeline failures with no indication of what went wrong.

While Crystal Forge has a first-admin registration flow (auto-detected when zero users exist via `/api/auth/setup-status`), there is no post-registration onboarding. The admin is dropped into an empty UI and left to figure out the required configuration order on their own.

## Goal

A guided setup wizard activates after the first admin registers and logs in for the first time. The wizard walks the admin through the essential configuration steps in the correct dependency order, ensuring the deployment pipeline is functional before they start using the system in production.

## Non-Goals

- This task does NOT implement persistent configuration health warnings across views (see TASK-186 — that task provides the reactive fallback; this task provides the proactive onboarding).
- This task does NOT implement the actual entity creation forms inline in the wizard — the wizard links to or embeds the existing creation views/forms. No new CRUD logic.
- This task does NOT add new entity types, database schema changes for domain entities, or new API mutations beyond what's needed for wizard state tracking.
- This task does NOT handle multi-tenant onboarding — this is for the first admin of a single Crystal Forge instance.
- This task does NOT add email/external notification for setup completion.
- This task does NOT change existing registration flow behavior — it extends what happens AFTER registration.

## Proposed Wizard Flow

The wizard guides the admin through these steps in dependency order (reflecting the pipeline chain):

### Step 1: Create Your First Environment
- Explain what environments are (organizational units for systems, builders, caches)
- Link to Environments page or embed the existing creation form
- Validation: at least one environment exists (check via API count)

### Step 2: Add a Flake
- Explain what flakes represent (source of NixOS configurations)
- Warn about git authentication requirements (SSH key or .netrc may be needed)
- Link to Flakes page or embed the existing add-flake form
- Validation: at least one flake exists and is being polled

### Step 3: Register a Builder
- Explain what builders do (process build jobs for evaluated derivations)
- Guide through keypair generation (builders view already supports this)
- Emphasize assigning the builder to the environment from Step 1
- Validation: at least one builder exists AND is assigned to an environment

### Step 4: Configure a Cache Destination
- Explain the role of caches (agents pull built store paths from cache)
- Guide through S3/Attic/Nix cache configuration options
- Emphasize assigning the cache to the environment from Step 1
- Mention optional signing key configuration
- Validation: at least one cache destination exists AND is assigned to an environment

### Step 5: Register a System
- Explain systems (managed NixOS hosts)
- Link system to the flake from Step 2 and environment from Step 1
- Explain public key requirement and deployment policy options (manual vs auto_latest vs pinned)
- Validation: at least one system exists with a flake_id and environment_id set

### Step 6: Deploy the Agent (Informational)
- Explain that the CF agent NixOS module must be enabled on target hosts
- Show the NixOS module configuration snippet for `services.crystal-forge.client`
- Explain agent heartbeat mechanism and how the server detects connected agents
- This step is informational only — the admin completes it outside the UI
- Validation: none (acknowledgment only, with a "I understand" or "Mark complete" button)

### Completion Screen
- Summary of what was configured with checkmarks
- Link to the dashboard (which should now show meaningful data)
- Note that the first evaluation begins automatically when the flake is polled
- "Get Started" button that navigates to dashboard and marks wizard complete

## Architectural Constraints

- **Wizard state tracking**: Use a derived approach consistent with the existing `setup-status` pattern in `auth_status.rs`. Extend or create a new endpoint (e.g., `GET /api/v1/admin/setup-progress`) that checks entity counts (environments, flakes, builders with env assignments, caches with env assignments, systems with flake_id). The wizard completion state can be derived: if all steps have at least one entity, the wizard is "complete." Additionally, persist a `setup_wizard_dismissed` flag so experienced admins can skip it permanently — this could be a user preference column on the `users` table or a simple `server_settings` key-value table (new migration required for either approach).
- **Wizard route**: Add a new `/setup` route in the `Route` enum in `routes.rs`. Place it after `#[end_layout]` (outside `AppShell`) since the wizard is a standalone full-screen experience, similar to login/register. Alternatively, place it inside `AppShell` if the sidebar should remain visible — decision TBD during implementation.
- **Wizard component**: New `views/setup.rs` component using a stepper/progress bar pattern. Each step is a sub-component. Step navigation is controlled by the wizard, with forward/back/skip controls.
- **Auto-redirect**: After first-admin registration, the registration success handler should redirect to `/setup` instead of `/` (dashboard). This modifies `views/register.rs` redirect logic.
- **Re-entry**: The wizard should be re-accessible from Server Management (`/admin`) via a "Re-run Setup Wizard" link. This adds a small UI element to `views/admin.rs`.
- **Skip mechanism**: A "Skip Setup" button is always visible. Clicking it calls an API endpoint to persist the dismissal and navigates to the dashboard. The wizard does not reappear after dismissal unless re-triggered from admin settings.
- **Step validation**: Each step queries the relevant entity count from the API (can reuse the health endpoint from TASK-186 if available, or make lightweight count queries). Steps show a green checkmark when their validation passes, regardless of whether the admin completed the step through the wizard or through normal navigation.
- **Frontend patterns**: Follow existing patterns — `web_sys` fetch for API calls, `use_context::<Signal<AppState>>()` for auth state, Dioxus RSX for rendering. Style consistently with existing views using `theme::` presets.
- **No new CRUD endpoints**: The wizard reuses existing entity creation endpoints. It only needs a read-only progress/status endpoint and a dismiss endpoint.

## Dependencies

- **Soft dependency on TASK-186**: If TASK-186 is completed first, the wizard can reuse the `GET /api/v1/admin/config-health` endpoint for step validation. If not, the wizard needs its own lightweight status checks (simple count queries). Either approach works — they are not blocking dependencies.

## Impact Areas

- `packages/web-ui/src/views/` — new `setup.rs` wizard view
- `packages/web-ui/src/views/mod.rs` — module registration
- `packages/web-ui/src/routes.rs` — new `/setup` route
- `packages/web-ui/src/views/register.rs` — post-registration redirect change
- `packages/web-ui/src/views/admin.rs` — "Re-run Setup Wizard" link
- `packages/default/src/handlers/api/` — new setup-progress endpoint (or extend auth_status)
- `packages/default/src/bin/server.rs` — route registration for new endpoint
- `packages/default/migrations/` — new migration if adding `setup_wizard_dismissed` column or settings table
- `packages/default/src/models/` or `packages/default/src/queries/` — setup progress query

## Risk Level

**Medium-High** — This is a new standalone feature with a new view, new route, and potentially a new migration. The wizard's UX quality directly impacts first impressions. Risk areas: (1) the wizard must stay in sync with actual entity creation flows — if those change, the wizard can become stale; (2) the redirect-after-registration change touches the auth flow; (3) the skip/dismiss persistence mechanism needs a design decision (user column vs settings table).

## Verification Plan

- **Tier 0**: `cargo fmt -- --check`, `cargo clippy -- -D warnings`, `cargo test` (targeted: setup-progress endpoint, wizard dismiss endpoint)
- **Tier 1**: Start full stack (`server-stack up`), verify:
  - Fresh instance: after registering first admin, user is redirected to `/setup` wizard
  - Wizard shows 6 steps with progress indicator
  - Each step displays explanation text and a link/button to the relevant configuration page
  - Completing a step (creating the entity in another tab or via the link) and returning to the wizard shows the step as completed (green checkmark)
  - Steps can be navigated forward, backward, and skipped
  - "Skip Setup" dismisses the wizard permanently and redirects to dashboard
  - Wizard does not reappear on subsequent logins after dismissal or completion
  - "Re-run Setup Wizard" link appears in Server Management for admins
  - Non-admin users never see or get redirected to the wizard
  - Wizard renders correctly on various viewport sizes (responsive)
- **Tier 2**: `nix flake check` — required since new API endpoint and potentially new migration affect the server package
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A `/setup` route exists and renders a multi-step wizard view with 6 steps: Environment, Flake, Builder, Cache, System, Agent.
- [ ] #2 After first-admin registration completes, the user is automatically redirected to `/setup` instead of the dashboard.
- [ ] #3 Each wizard step displays: (a) a clear explanation of what the entity is and why it matters, (b) a link or embedded form to create the entity, and (c) a visual completion indicator (checkmark) that updates dynamically when the entity exists.
- [ ] #4 A stepper/progress bar component shows all 6 steps with the current step highlighted and completed steps marked with checkmarks.
- [ ] #5 Step validation is derived from real entity counts via API — if an environment already exists (e.g., created outside the wizard), Step 1 shows as complete automatically.
- [ ] #6 Step 3 (Builder) validation requires at least one builder AND that it is assigned to an environment. Step 4 (Cache) validation requires at least one cache destination AND that it is assigned to an environment.
- [ ] #7 Step 6 (Agent) is informational only — it displays the NixOS module configuration snippet for `services.crystal-forge.client` and has a 'Mark as understood' acknowledgment button.
- [ ] #8 The wizard completion screen summarizes all configured entities and provides a 'Get Started' button that navigates to the dashboard.
- [ ] #9 A 'Skip Setup' button is visible on every wizard step. Clicking it persists the dismissal (so the wizard doesn't reappear) and navigates to the dashboard.
- [ ] #10 After the wizard is completed or skipped, subsequent logins do NOT redirect to `/setup` — the admin goes directly to the dashboard.
- [ ] #11 A 'Re-run Setup Wizard' link is available in the Server Management view (`/admin`) for admin users, allowing them to re-access the wizard at any time.
- [ ] #12 The wizard is accessible ONLY to admin users. Non-admin users navigating to `/setup` are redirected to the dashboard or shown an access denied message.
- [ ] #13 A `GET /api/v1/admin/setup-progress` endpoint (or equivalent) returns the completion status of each wizard step based on entity counts. Returns 403 for non-admin users.
- [ ] #14 A `POST /api/v1/admin/setup-wizard/dismiss` endpoint (or equivalent) persists the wizard dismissal so it doesn't reappear on login. A corresponding mechanism allows re-enabling it from admin settings.
- [ ] #15 Unit tests exist for the setup-progress endpoint covering: empty instance (no steps complete), partially configured, fully configured, and 403 for non-admin access.
- [ ] #16 The wizard view is responsive and renders correctly on viewport widths from 768px to 1920px.
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Moved to To Do by maintainer request on 2026-03-13 for immediate execution next.

LOCK: OpenCode on reckless in ~/code/crystal-forge/TASK-187-first-time-admin-setup-wizard

Sprint-Ready review: task includes objective ACs, non-goals, architecture constraints, verification plan, impact areas, and risk/dependency notes. Proceeding with implementation.

Implementation progress update (OpenCode, reckless): Added `/setup` wizard view + route, admin-only backend progress/dismiss/ack endpoints, login/register redirect behavior, and Admin 'Re-run Setup Wizard' control.

Added migration `0097_add_setup_wizard_user_flags.sql` with `users.setup_wizard_dismissed` and `users.setup_wizard_agent_acknowledged`; added users query helpers and API DTOs/client methods.

Verification run: `nix develop -c env SQLX_OFFLINE=true cargo check -p crystal-forge` (pass), `nix develop -c cargo check` in `packages/web-ui` (pass), `nix develop -c env SQLX_OFFLINE=true cargo test -p crystal-forge setup_wizard` (4 tests pass), `nix flake check` (pass).

SQLx sync completed: started DB with `nix run .#devScripts.db-only -- -D up`, then `nix develop -c cargo sqlx prepare` in `packages/default` (pass).

Known verification caveats: `cargo fmt -- --check` reports extensive pre-existing formatting diffs outside task scope; `cargo clippy -D warnings` currently not a clean gate in this branch/environment due existing repository warning baseline and toolchain/cache inconsistencies.
<!-- SECTION:NOTES:END -->

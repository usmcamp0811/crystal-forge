---
id: TASK-186
title: Admin Configuration Health Warnings — Pipeline Readiness Alerts
status: Backlog
assignee: []
created_date: '2026-03-13 01:16'
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
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

When a new admin stands up Crystal Forge for the first time, they create a system and a flake and expect things to work — but they don't. There is no feedback about *why*. The deployment pipeline has multiple stages (commit detection → evaluation → policy check → build → cache push → agent deployment), and a missing configuration at any stage causes a silent failure. The admin has no way to know what's misconfigured without independently understanding the full pipeline architecture.

## Desired Outcome

Admin users receive clear, actionable warnings about configuration gaps that will prevent the deployment pipeline from functioning. Warnings appear in three locations:

1. **Dashboard** — A configuration health summary widget showing overall pipeline readiness and listing unresolved issues
2. **Contextual views** — Inline warnings on the specific entity views where the missing config is relevant (e.g., the Systems view warns about missing builders, the Flakes view warns about missing cache)
3. **Global notification bar** — A persistent top-of-page banner visible across all views (dismissible per-session) that summarizes critical configuration gaps until resolved

Warnings are visible **only to Admin users**. Non-admin users (Operators, Viewers) see normal empty states.

## Configuration Health Checks

The following pipeline readiness checks should be implemented, covering every critical checkpoint in the eval-build-deploy flow:

### Global / Dashboard Level
1. **No Flakes configured** → "No flakes are being watched. Add a flake to begin evaluating NixOS configurations."
2. **No Environments created** → "No environments exist. Environments are required to organize systems, builders, and caches."
3. **No Builders registered** → "No builders are registered. Derivations will be evaluated but never built."
4. **No Cache Destinations configured** → "No cache destinations configured. Builds will succeed but agents won't be able to pull deployments."

### System View (contextual, per-system or list-level)
5. **System has no flake_id** → "This system is not linked to a flake. It won't be included in evaluations."
6. **System has no connected agent** → "No agent heartbeat detected. This system cannot receive deployments." (Already enforced by `require_cf_agent` policy, but the UI should surface it clearly)

### Environment View (contextual, per-environment)
7. **Environment has no builder assigned** → "No builder is assigned to this environment. Builds for systems in this environment won't be processed."
8. **Environment has no cache destination assigned** → "No cache destination is assigned to this environment. Builds for this environment won't be deployable."

### Flakes View (contextual)
9. **Flake has evaluation errors on latest commit** → "Latest evaluation failed. Check flake configuration." (informational, not a config gap per se)

## Technical Notes

- A new server-side API endpoint (e.g., `GET /api/v1/admin/config-health`) should aggregate all checks and return a structured health status response. This avoids N+1 queries from the frontend.
- The frontend should call this endpoint on dashboard load and cache the result for contextual views during the session.
- Contextual warnings on entity views can supplement with entity-specific checks (e.g., per-system flake_id presence is already available from the system list endpoint).
- The global notification bar should be dismissible per browser session (localStorage flag) but reappear if the health status changes.
- Warnings should include actionable links (e.g., "No builders registered" links to the Builders page with a prompt to create one).

## References

- Deployment pipeline flow: `docs/eval-build-deploy-flow.md`
- Entity relationships: see database migrations in `packages/default/migrations/`
- Existing setup-status endpoint pattern: `handlers/api/auth_status.rs` (`/api/auth/setup-status`)
- Sidebar/view structure: `packages/web-ui/src/components/layout/sidebar.rs`
- Dashboard view: `packages/web-ui/src/views/dashboard.rs`
- Role checking: `packages/web-ui/src/services/auth.rs` (`is_admin()`)
<!-- SECTION:DESCRIPTION:END -->

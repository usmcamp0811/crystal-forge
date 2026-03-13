---
id: TASK-119
title: 'Dashboard: Replace Mock Data with Backend API (With Safe Fallback)'
status: Done
assignee: []
created_date: '2026-02-23 03:35'
updated_date: '2026-03-13 01:24'
labels: []
dependencies: []
priority: medium
ordinal: 69000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

The dashboard currently renders mock/static data in the UI layer. This prevents:

* Accurate system/environment visibility
* Real operational metrics
* Confidence in RBAC-scoped behavior
* Realistic end-to-end verification

We need the dashboard to consume backend API data while preserving a mock fallback for environments without a database.

## Goal

Update the dashboard view to:

1. Fetch real data from the backend API.
2. Respect auth context and environment scoping.
3. Gracefully fall back to deterministic mock data if:

   * The database is not present
   * The API returns an error
   * The backend is running in dev/mock mode

The dashboard must remain usable in development and Nix checks without requiring a running DB.

## Non-Goals

* No new metrics aggregation logic in this issue.
* No backend schema changes.
* No redesign of dashboard layout.
* No business logic embedded directly in UI components.

## Architectural Constraints

* UI consumes backend-provided data only.
* No policy logic in views.
* No direct DB access from UI.
* Role and environment scoping must be enforced server-side.
* Mock fallback must live in a clearly separated adapter layer.
* No infrastructure imports in UI layer.

## High-Level Design

Introduce a small data abstraction layer in `web-ui`:

```
dashboard/
  ├── api.rs            # API client functions
  ├── adapter.rs        # Real vs mock resolution logic
  ├── models.rs         # DTOs
  └── view.rs           # Existing UI view
```

### Flow

1. `view.rs` calls `adapter::load_dashboard()`
2. Adapter:

   * Attempts API fetch
   * If success → return real data
   * If failure / dev mode → return mock data
3. View renders same DTO regardless of source

Single data contract, multiple providers.

## API Contract (Expected)

Dashboard endpoint example:

```
GET /api/dashboard
```

Response example:

```json
{
  "total_systems": 14,
  "environments": [
    { "name": "prod", "system_count": 6 },
    { "name": "staging", "system_count": 4 }
  ],
  "recent_deployments": 5
}
```

If the endpoint does not exist yet, this issue includes wiring it to existing backend aggregates.

## Acceptance Criteria

* Dashboard calls backend API when available.
* UI renders real data when DB exists.
* If backend returns 500 / network failure → UI falls back to mock data.
* Mock fallback is deterministic and clearly marked in code.
* No duplicate business logic in UI.
* No breaking changes to auth/session behavior.
* Works in both `local` and `oidc` modes.

## Verification Plan

Backend:

```
nix build .#checks.x86_64-linux.default
```

UI:

```
nix build .#checks.x86_64-linux.web-ui
nix develop -c cargo test --package web-ui dashboard
```

Manual:

1. Run backend with DB → verify real counts render.
2. Run backend without DB → verify mock data renders.
3. Disable backend entirely → verify fallback still renders.
4. Confirm role-scoped user only sees permitted environments.

## Edge Cases to Cover

* 401/403 response (should redirect to login, not fallback)
* 500 response (should fallback)
* Empty DB (should render empty state, not fallback)
* Slow API (show loading state)

## Risk Level

Medium
Touches core UI path and API integration, but isolated behind adapter.

## Impact Areas

* Web UI
* API layer
* Auth-aware data consumption
<!-- SECTION:DESCRIPTION:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
LOCK: opencode on gray in /home/mcamp/code/crystal-forge/TASK-119-dashboard-api-fallback

MR: https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/131

Verification: nix develop -c cargo test dashboard (packages/web-ui) passed; nix build .#checks.x86_64-linux.web-ui passed.
<!-- SECTION:NOTES:END -->

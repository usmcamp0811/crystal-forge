---
id: TASK-74
title: Add runtime detection for dev mode banner visibility
status: Done
assignee: []
created_date: '2026-02-20 15:05'
updated_date: '2026-03-13 01:24'
labels:
  - ui
  - auth
  - devex
dependencies: []
priority: medium
ordinal: 66000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Problem: DevModeBanner currently renders unconditionally in AppShell, meaning it appears even when AUTH_MODE=oidc/production.

Goal: Only show the dev mode warning banner when AUTH_MODE=dev is actually active on the server.

Solution Options:
1. Add auth_mode field to /api/v1/status endpoint (recommended - clean and explicit)
2. Probe /api/auth/dev/login endpoint (returns 200 in dev mode, 404 otherwise)
3. Add compile-time build flag (requires build-time coordination, less flexible)

Recommended: Option 1 - extend /api/v1/status to include:
{
  "service": "Crystal Forge",
  "auth_mode": "dev" | "oidc",
  ...
}

Then in UI, fetch status on mount and conditionally render DevModeBanner.

Acceptance Criteria:
- Server /api/v1/status endpoint includes auth_mode field
- DevModeBanner only renders when auth_mode === 'dev'
- Banner does NOT appear when AUTH_MODE=oidc or production
- No flash of banner before detection completes (use suspense/loading state)
<!-- SECTION:DESCRIPTION:END -->

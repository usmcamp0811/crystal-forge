---
id: TASK-336.3
title: 'Admin Server: add server info API endpoint (version/uptime/db/auth/TLS)'
status: Backlog
assignee: []
created_date: '2026-06-20 02:59'
labels:
  - admin
  - server
  - api
  - backend
milestone: 'm-20: Design Parity Missing Surfaces'
dependencies: []
priority: medium
ordinal: 309000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The Admin Server tab shows Build Info and Authentication cards. These currently display "not implemented yet" because no backend endpoint exposes server version, commit hash, uptime, database status/size, auth mode, OIDC issuer, active session count, or TLS certificate expiry. Add a `/api/admin/server-info` endpoint and matching client model so the Server tab can display real values.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A backend endpoint returns server version, commit hash, uptime, database status and size, auth mode, OIDC issuer URL, active session count, and TLS cert expiry
- [ ] #2 The Admin Server tab Build Info and Authentication cards display real values from this endpoint
<!-- AC:END -->

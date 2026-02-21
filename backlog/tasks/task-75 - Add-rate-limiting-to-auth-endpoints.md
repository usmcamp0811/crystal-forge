---
id: TASK-75
title: Add rate limiting to auth endpoints
status: Backlog
assignee: []
created_date: '2026-02-21 04:17'
labels:
  - enhancement
dependencies:
  - TASK-65.3
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Add rate limiting to authentication endpoints (login, logout, OIDC callback) to prevent abuse and DoS attacks. Priority: logout endpoint (10 req/min per IP), then login endpoints (5 req/min per IP).
<!-- SECTION:DESCRIPTION:END -->

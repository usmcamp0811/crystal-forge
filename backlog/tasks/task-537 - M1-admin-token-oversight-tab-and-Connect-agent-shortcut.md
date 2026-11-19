---
id: TASK-537
title: M1 admin token oversight tab and Connect agent shortcut
status: Backlog
assignee: []
created_date: '2026-11-19'
labels: [security, agent-access, ui]
parent_task_id: TASK-534
dependencies:
  - TASK-534.3
references:
  - TASK-534
  - TASK-534.3
  - docs/knowledge/security/agent-access-tokens-mcp-remediation-context.md
priority: medium
---

## Description
<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

TASK-534.3 delivers the owner workflow (Profile card, create, one-time reveal,
details, rotate, revoke). Two M1 parts of CF-AGENT-ACCESS-001 v1.1 remain:

- Spec 10.5: an Admin Access Tokens tab for oversight only. It lists token
  metadata across accounts, with totals and filters; revokes one token; revokes
  all tokens of a user (`GET /admin/access-tokens`,
  `DELETE /admin/access-tokens/:id`,
  `POST /admin/users/:id/access-tokens/revoke-all`). An Admin never creates a
  token for another user and never sees a secret.
- Spec 10.1.1: a Connect agent shortcut on Fleet CVEs and POA&M views that opens
  the Profile token-creation flow with a non-authoritative environment hint. The
  server resolves permitted environments and scopes again; the shortcut never
  issues a token or places one in a URL.

## Desired outcome

Both parts exist with session plus CSRF, paged admin-authorized queries,
audit events for admin revocation, and Access scenario coverage. The Admin
Access Inspector (spec 10.7) and the MCP status card (spec 10.6) belong to M2.
<!-- SECTION:DESCRIPTION:END -->

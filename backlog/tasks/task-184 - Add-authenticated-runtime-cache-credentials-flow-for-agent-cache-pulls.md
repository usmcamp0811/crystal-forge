---
id: TASK-184
title: Add authenticated runtime cache credentials flow for agent cache pulls
status: Backlog
assignee: []
created_date: '2026-03-12 00:13'
labels:
  - security
  - agent
  - cache
  - multi-tenant
  - runtime
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Problem:
Agents currently receive runtime cache metadata (URL/type/public key) via heartbeat response, but do not receive cache credentials (Attic token or S3 auth material). Builder-side cache pushes do use cache credentials from DB-backed cache destinations, but agent pull path is credentialless.

Desired outcome:
When a deployment target requires pulling from authenticated/private caches, agents can securely fetch required cache credentials and successfully perform pull/deploy operations without exposing cross-tenant secrets.

Scope:
- Define secure server->agent credential delivery for runtime cache pulls (short-lived/scoped tokens preferred).
- Ensure credentials are scoped to the agent environment/tenant and authorized resources only.
- Update agent deployment/cache pull path to use provided credentials for Attic/S3/private cache pulls.
- Preserve redaction and avoid leaking credentials in logs/API/UI.
- Add tests for authorized pull success and unauthorized cross-environment access denial.

Non-goals:
- Reworking builder push architecture.
- Long-term secret manager integration beyond minimal secure runtime flow.

Risk:
High (security-sensitive secret handling, tenant boundary enforcement).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Agent can pull from authenticated cache destinations (Attic/S3/private endpoint) when authorized for the environment.
- [ ] #2 Agent does not receive credentials for caches outside its authorized environment scope.
- [ ] #3 Credentials are not returned by normal admin/list APIs and are redacted from logs/telemetry.
- [ ] #4 Credential delivery uses scoped/ephemeral mechanism (or documented secure fallback) with rotation/expiry behavior defined.
- [ ] #5 Automated tests cover positive pull path and negative cross-environment credential access attempts.
- [ ] #6 Existing builder cache push path remains functional.
<!-- AC:END -->

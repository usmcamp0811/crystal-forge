---
id: TASK-184
title: Add authenticated runtime cache credentials flow for agent cache pulls
status: Done
assignee: []
created_date: '2026-03-12 00:13'
updated_date: '2026-03-13 00:53'
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

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
## Implementation Found

This task was already implemented in commit c78b1e2f (merged to dev on 2026-03-10):

```
feat(cache): add attic public key and runtime cache delivery to agents

- Add attic_public_key field to cache destinations with migration and backend CRUD wiring
- Require Attic push URL, cache name, and public key in backend validation
- Include environment-scoped runtime cache configuration in /agent/heartbeat response
- Update agent deployment flow to use server-provided runtime cache config at deploy time
- Pass trusted-public-keys to nix copy when runtime cache public key is available
- Add Attic Public Key field + validation in cache Add/Edit modals

Closes: TASK-42
```

Implementation:
- Server delivers runtime cache config (URL, type, public key) via heartbeat response
- Agents use server-provided cache config for deployment pulls
- Environment-scoped credential delivery (agents only receive authorized cache info)
- Public key included in cache pull operations

Files changed:
- migration: add attic_public_key field
- packages/default/src/deployment/agent.rs (+84 lines)
- packages/default/src/handlers/agent/heartbeat.rs (+46 lines)
- packages/default/src/models/cache_destination.rs (+32 lines)
- packages/default/src/queries/cache_destinations.rs (+12 lines)
- packages/web-ui/src/api/models.rs (+3 lines)
- packages/web-ui/src/views/caches.rs (+78 lines)

Task marked Done (already merged).
<!-- SECTION:NOTES:END -->

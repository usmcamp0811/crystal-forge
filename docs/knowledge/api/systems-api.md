---
type: API
title: "Systems API"
description: "Lists the systems endpoints with role requirements, the transactional PATCH rules, compliance bundle assignment scope, query parameters, and the manual deployment request contract including idempotent request IDs and expiry rules."
tags:
  - crystal-forge
  - api
  - systems
  - deployments
  - compliance
implementation_status: implemented
generated:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-03T22:54:29-05:00
sources:
  - id: origin
    resource: "Crystal Forge repository file docs/specs/02-backend-api.md at commit 3b23d36f"
    title: "Backend API Specification"
---

# Systems API

Systems are the NixOS machines CF manages.

## Endpoints

| Method | Endpoint | Role | Description |
|--------|----------|------|-------------|
| GET | `/systems` | Viewer+ | List all systems |
| POST | `/systems` | Operator+ | Register new system |
| GET | `/systems/:id` | Viewer+ | Get system details |
| PATCH | `/systems/:id` | Operator+ | Update system |
| DELETE | `/systems/:id` | Admin+ | Remove system |
| POST | `/systems/:id/deploy` | Operator+ | Trigger deployment |
| POST | `/systems/:id/rollback` | Operator+ | Rollback generation |
| POST | `/systems/:id/sync` | Operator+ | Sync flake |
| GET | `/systems/:id/deployments` | Viewer+ | Deployment history |
| GET | `/systems/:id/logs` | Viewer+ | Deployment logs |
| GET | `/systems/:id/evaluated-options` | Viewer+ | Read cached revision options |
| GET | `/systems/:id/evaluation-summary` | Viewer+ | Read cached scalar revision summary |
| GET | `/systems/:id/evaluation-module-sources` | Viewer+ | Read cached bounded module-source pages |
| POST | `/systems/:id/config-inspections/:revision` | Admin | Queue or reuse targeted Config inspection |
| POST | `/systems/:id/evaluations/:revision` | Admin | Explicit whole-commit evaluation prerequisite |

`PATCH /systems/:id` applies authorization, environment and system locking,
metadata changes, and response construction in one transaction. An Operator
must have current membership in both the source and destination environments.
Admin is not environment-scoped. The transaction re-reads the active user,
roles, and memberships after it acquires the environment and system locks, so a
concurrent revocation prevents the move. Unknown and unauthorized environments
use the same not-found behavior for scoped callers. The success body is built
from the transaction's updated row before commit.

## Compliance bundle assignment scope

`GET /systems/:id/compliance` returns each bundle lineage that has an active
system or environment assignment for the system. Its per-bundle
`assigned_bundle_version_id` and `assignment_mode` identify the governing
immutable assignment snapshot. A system assignment takes precedence over an
environment assignment for the same bundle. The catalog's
`current_published_version_id` and `current_draft_version_id` do not control
system applicability or retarget an existing assignment. The bundle summary's
catalog version and current-version counts remain catalog metadata; they are
not the system's assigned version.

`GET /compliance/bundles/:id/systems/:system_id/evidence` without `version_id`
uses that system's effective active assignment version and its exact policy
membership and overlays. Without an active assignment, it returns not-found;
inactive assignment history is not authority. With `?version_id=<uuid>`, the
request inspects precisely that version only when the system's effective
assignment targets it. It does not substitute the global catalog version.

`report_only` changes deployment enforcement, not compliance evidence or
remediation. A composite policy in either mode produces an exact-target
assessment and ordered rule results. A failing report-only assessment remains
FAIL and can support the same stable finding, waiver, POA&M creation/linking,
and verification as an enforced FAIL. Deployment authorization selects only
enforced composite assessments. A POA&M does not turn FAIL into PASS. Changing
assignment mode does not replace the stable finding or erase POA&M history;
current actions require an assessment for the effective mode and exact target.
An older assessment does not become current again if the assignment mode
changes back; currentness also requires evidence created under the active
assignment snapshot.

`GET /compliance/bundles/:id/systems` without `version_id` remains a
single-version convenience alias for the bundle's current published (or draft)
version. It does not combine systems pinned to other versions. The exact
`?version_id=<uuid>` form lists only systems whose effective assignment targets
that version. The catalog `applicable_system_count` keeps its current-version
unit, consistent with the unversioned bundle-systems alias. A lineage-wide
assigned-system count would need a separately named contract.

## Query Parameters

```bash
# Filter by environment
GET /api/v1/systems?environment=prod

# Filter by status
GET /api/v1/systems?status=online

# Search by name
GET /api/v1/systems?search=web
```

## Example: List Systems

**Request:**
```bash
GET /api/v1/systems?environment=prod
```

**Response:**
```json
{
  "data": [
    {
      "id": "sys-123",
      "name": "prod-web-01",
      "hostname": "prod-web-01.example.com",
      "environment_id": "env-prod",
      "environment_name": "Production",
      "status": "online",
      "last_heartbeat": "2024-01-15T10:30:00Z",
      "deployed_flake": "github:org/configs",
      "deployed_commit": "abc1234"
    }
  ],
  "pagination": {
    "page": 1,
    "per_page": 20,
    "total": 5
  }
}
```

## Example: Trigger Deployment

**Request:**
```bash
POST /api/v1/systems/sys-123/deploy
{
  "commit_sha": "def56789abcdef0123456789abcdef0123456789",
  "action": "convert_to_manual",
  "request_id": "7ce63e03-935d-4903-ae9f-903f14242cab"
}
```

Manual deployment requests also accept `action` and `request_id`. The action is
`deploy`, `continue_auto_latest`, or `convert_to_manual`. An `auto_latest`
system requires one of the latter two explicit outcomes. New clients reuse one
UUID `request_id` for retries. The UUID is bound immutably to the system, full
commit SHA, and action. A conflicting reuse returns HTTP 409 before policy
conversion. If conversion succeeds but queueing fails, the response reports the
persisted manual policy separately from the failed deployment state. Legacy
clients that omit `request_id` use a server-derived 24-hour replay window.

A deployment target expires after two hours if no matching agent state report
arrives. For 24 hours after terminal completion, an `expired` deployment remains
eligible for exact evaluation-generation retention when system, store path,
derivation, commit, configuration, artifact, and report timestamp all match. A
delayed successful activation does not change the deployment row from `expired`
to `succeeded`; it records a correlated `cf_deployment_succeeded` state event.
Failed and superseded deployments are not eligible for this delayed correlation
or retention. Matching selects the newest same-path deployment issued no later
than the report before it checks status, so a newer failed or superseded request
cannot fall back to older work.

**Response:**
```json
{
  "status": "accepted",
  "policy": "manual",
  "conversion": "converted",
  "deployment": "queued",
  "deployment_id": "79ea0220-5715-49ce-8e73-74c09a5ea289",
  "message": "System policy is manual. Deployment requested"
}
```

## Related concepts

- [Flakes API and Evaluation and Flake Snapshot API](flakes-and-evaluation-snapshot-api.md)
- [Backend API overview, error codes, and WebSocket streaming](api-overview-errors-and-streaming.md)
- [API authentication, sessions, and role-based authorization](../security/api-authentication-and-authorization.md)

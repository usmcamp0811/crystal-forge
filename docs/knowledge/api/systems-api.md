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
implementation_status: partial
generated:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-03T22:54:29-05:00
sources:
  - id: origin
    resource: "Crystal Forge repository file docs/specs/02-backend-api.md at commit 3b23d36f"
    title: "Backend API Specification"
---

# Systems API

Systems are registered managed machines, externally deployed machines, or
reference configurations. See [management modes and configuration assurance](../concepts/management-modes-and-configuration-assurance.md)
for authority, migration and current implementation gaps. TASK-508 integration
verification and final UI/API alignment remain pending.

## Endpoints

| Method | Endpoint | Role | Description |
|--------|----------|------|-------------|
| GET | `/systems` | Viewer+ | List all systems |
| POST | `/systems` | Operator+ | Register new system |
| GET | `/systems/:id` | Viewer+ | Get system details |
| GET | `/systems/:id/configuration-assurance` | Viewer+ | Read exact configuration evidence and bounded history |
| GET | `/systems/:id/guest-observations/latest` | Viewer+ | Read latest stored parent report or JSON null |
| PATCH | `/systems/:id` | Operator+ | Update system |
| DELETE | `/systems/:id` | Admin+ | Remove system |
| POST | `/systems/:id/deploy` | Operator+ | Trigger deployment |
| POST | `/systems/:id/rollback` | Operator+ | Rollback generation |
| POST | `/systems/:id/sync` | Operator+ | Sync flake |
| POST | `/systems/:id/cve-scan` | Operator+ | Request the server-resolved scan target with an optional expected-ID precondition |
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

## Management-mode fields and writes

All paths on this page have the `/api/v1` prefix. List/detail responses expose
`management_mode` (`managed`, `external`, `reference`) and nullable
`parent_host_id`. `GET /systems?management_mode=reference` filters before totals
and pagination. The list population is active registrations, not just machines.

Create defaults an omitted mode to managed and requires a valid public key for
that mode. External/reference create omits `public_key` (or sends null), requires
`flake_name` and explicit `system_configuration_name`, and uses `hostname` as a
unique resource name. Reference names can be descriptive. Different UUIDs/names
can select the same configuration. No fake agent key is generated.

Update preserves omitted mode. `parent_host_id` has three meanings: omitted
preserves, null clears, UUID sets. Only external resources can set a parent;
moving out of external clears the association. The parent must be active,
managed and in the same environment. Clear child associations before changing
a parent's mode/environment. Metadata writes audit the mode and parent changes
without credentials in the same transaction.

Initial external/reference-to-managed update explicitly supplies managed mode,
a new valid `public_key`, and `deployment_policy: "manual"`. It queues no deploy.
Later explicit edits can select automatic/pinned policy. A changed-mode PATCH
response includes `management_transition` with `previous_mode`,
`management_mode`, and `deployment_policy`; ordinary reads/unchanged-mode edits
omit that field. Moving out of
managed removes credentials/targets, supersedes pending work and invalidates
request reservations atomically. Old request IDs cannot revive revoked work.

The PATCH DTO is not a generic sparse patch: existing required fields such as
`hostname` and `deployment_policy` still apply. Nonmanaged edits require a real
flake and explicit configuration. Never infer omitted-field preservation for
fields other than those with a documented PATCH contract.

Registration metadata is independent of management mode. Create/list/detail/edit
responses expose `tags` (default `[]`) and nullable `description` (operator
notes). Create and edit persist both fields in every mode. Managed create also
persists optional `fqdn` and `reachability` (default `direct`). Nonmanaged create
does not retain address/transport input. Reachability accepts `direct` or `pull`;
it is not a new deployment policy.

| PATCH field | Omitted | Explicit null | Supplied value |
| --- | --- | --- | --- |
| `tags` | Preserve | Clear to `[]` | Trim, remove blanks, deduplicate case-sensitively in first-occurrence order |
| `description` | Preserve | Clear | Trim; blank becomes null |
| `fqdn` | Preserve | Clear | Trim; blank clears |
| `reachability` | Preserve | Reset to `direct` | Validate trimmed `direct` or `pull` |

Config synchronization preserves these operator fields. Migration `0319` adds
the persistent tags/description columns; the existing address fields remain in
use. Tags also participate in server search and exact `tag` filtering before
pagination and totals.

The nullable `systems.agent_enrolled_at` is database-owned, not client-editable
enrollment intent. Migration `0318` stamps only nonmanaged-to-managed conversion
with `clock_timestamp()` after the system-row lock. Legacy null epochs remain
unchanged; ordinary key rotation, metadata writes and config sync preserve the
epoch. Both signed write channels recheck key, epoch, mode and activity under
the transaction lock. A pre-conversion request cannot write after re-enrollment
even if the key is reused. Until a fresh authorized full observation exists,
old runtime states cannot regain Current authority. The first post-enrollment
heartbeat forces a full row even when runtime fields are unchanged.

The list, detail, create and update responses report `health_status:
"unknown"` (and `deployment_status: "unknown"`) for external and reference
registrations. `unknown` is not an outage: no agent reports for these
resources, and a reference configuration is not a machine. Managed health keeps
its heartbeat-derived values (`healthy`, `warning`, `critical`, `offline`).

Migration `0320` preserves inactive managed detail telemetry filtered by mode
and enrollment epoch. `is_active = false` alone does not classify health as
offline. Current authority and signed writes still require activity.

## Scoped CVE scan intent

`POST /api/v1/systems/:id/cve-scan?expected_derivation_id=<id>` requires
`RoleMutator`, matching CSRF credentials, and current environment access. Scoped
callers hold the environment/registration scope lock through admission. The
optional query precondition narrows the server-resolved target; it does not
authorize the client-supplied derivation. A changed eligible target returns
409 `scan_target_changed` before queueing. An absent or blocked target returns
the existing ineligible response rather than queueing a substitute.

Nonmanaged requests use the durable scan queue without local vulnix admission
requirements. Managed requests retain immediate local execution. The 202
response includes `scan_id` and `derivation_id`. Pending intent does not prove
execution. See [scan operations](cve-scan-operations-api.md).

## Configuration-assurance read

`GET /api/v1/systems/:id/configuration-assurance` performs database-only reads.
Admins can read all registrations; Operators/Viewers require current environment
membership. Missing/inaccessible resources return 404; unauthorized roles return
403; persistence failures return 500. Inactive resources retain readable history.

| Top-level field | Meaning |
| --- | --- |
| `system_id`, `management_mode`, `configuration_name`, `flake_id`, `branch` | Registration and effective configuration identity |
| `selected_revision` | Ready registered branch snapshot position-0 commit; null if unavailable |
| `evaluation` | `availability`, `state`, `attempt_id`, `error`, `completed_at` for the exact selection |
| `selected_artifact` | Exact selected revision/configuration derivation and latest build attempt, or null |
| `selected_scan` | Latest lifecycle for the selected artifact, including waiting/failed states |
| `latest_terminal_scan` | Latest completed/failed scan across retained revisions |
| `latest_good_scan` | Latest completed sealed schema-1 scan across retained revisions; may be historical |
| `history` | `limit_per_kind: 20`, newest-first `evaluations`, `builds`, `scans` arrays |

Revision objects contain `commit_id`, full `sha`, `committed_at`, and nullable
`message`. Evaluation availability is `available`, `removed`, or `unavailable`;
native discovery alone proves removal. State uses the native configuration
outcome or attempt state; a completed attempt alone cannot prove configuration
success.

Artifacts contain `derivation_id`, `revision`, `configuration_name`, `state`,
`derivation_path`, actual `output_path`, `expected_output_path`, `published_at`,
`error`, and nullable `build`. Build fields are `job_id`, `state`, `created_at`,
`started_at`, `completed_at`, and `failure_code`. Publication time means an exact
completed cache push, not current reachability or runtime activation.

Scan fields are `scan_id`, `derivation_id`, `revision`, `configuration_name`,
`state`, `created_at`, `completed_at`, `evidence_schema_version`, `cves`, `error`,
`diagnostics`, and `diagnostics_truncated`. Sealed counts contain `total`,
`critical`, `high`, `medium`, `low`: distinct non-whitelisted CVE IDs with current
catalog severity. Unknown severity can make total exceed the severity sum.
Unsealed/missing counts are null. Diagnostics retain at most 50 newest
receipt-ordered events (`occurred_at`, `level`, `event_type`, `message`); messages
and errors are redacted and bounded to 2,048 characters.

Optional values serialize as null; empty histories as `[]`. Histories follow
the current flake/configuration association, not every previous association of
the registration. This response contains no guest observations, deployment
attestation, Current authority, POA&M mutations, or execution secrets.

## Latest guest-observation read

`GET /api/v1/systems/:id/guest-observations/latest` uses Viewer+ environment
authorization (Admin unscoped), with 404 for hidden/missing resources and 403 for
unauthorized roles. It returns one object or JSON null. Object fields are `id`,
`guest_id`, `parent_host_id`, `environment_id`, `guest_identity`, `unit`,
`unit_active_state`, `runner_store_path`, `observed_at`, and `received_at`.
UUIDs are strings, row ID is an integer, and times are RFC 3339 strings.

Only an active external resource's current association to an active managed
parent in the same environment can return a row. Null also covers inactive
guests and detached/revoked associations; stored history remains retained.
This is a latest-only persisted read, not an append/history or synchronous
Refresh API. Collection time can be old; HTTP success does not imply freshness.
The current agent supplies only unit ActiveState and a null runner path.

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
GET /api/v1/systems?health_status=healthy

# Filter by management authority
GET /api/v1/systems?management_mode=reference

# Search by name
GET /api/v1/systems?search=web

# Filter by exact persisted tag
GET /api/v1/systems?tag=edge
```

## Example: List Systems

**Request:**
```bash
GET /api/v1/systems?environment=prod
```

The response is `PaginatedResponse<SystemSummary>` with top-level `items`,
`total`, `page`, and `per_page`. Each item has a UUID `id`, `hostname`, nullable
`system_configuration_name`, `environment`, `flake_id`, `parent_host_id`, and
`management_mode`, `tags`, and nullable `description`. Status fields are
`health_status`, `deployment_status`, and
nullable `pipeline_stage`; runtime metadata includes `nixos_version`,
`last_seen`, `fqdn`, `boot_id`, `heartbeat_interval_secs`, and
`effective_heartbeat_interval_secs`. `deployment_policy` and `cve_counts` are
separate fields. Configuration-only counts come from configuration-assurance,
not the legacy integer CVE fields. The response has no nested `pagination`
object, `name`, `status: "online"`, or inferred deployed revision.

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

# Backend API Specification

This document describes Crystal Forge's HTTP API. It's written for developers who need to understand how the backend works, what endpoints exist, and how to add new ones.

**Assumption:** You understand HTTP (GET, POST, etc.), REST APIs, and basic database concepts.

---

## API Overview

The API is a **REST API** that the frontend uses to talk to the backend.

**Base URL:** `http://localhost:8080/api/v1/`

### Request Format

- **Headers:** `Content-Type: application/json`
- **Body:** JSON for POST/PATCH requests
- **Authentication:** Cookie-based sessions

### Response Format

**Success:**
```json
{
  "data": {
    "id": "123",
    "name": "example"
  }
}
```

**Paginated:**
```json
{
  "data": [...],
  "pagination": {
    "page": 1,
    "per_page": 20,
    "total": 100
  }
}
```

**Error:**
```json
{
  "error": {
    "code": "NOT_FOUND",
    "message": "System not found"
  }
}
```

---

## Authentication

### How Sessions Work

1. User logs in (OIDC or Dev Mode)
2. Server creates session in database
3. Server sets `session_id` cookie in browser
4. Subsequent requests include the cookie
5. Middleware validates session

### Login Endpoints

| Method | Endpoint | Description |
|--------|----------|-------------|
| POST | `/auth/login` | Local email/password login |
| POST | `/auth/logout` | Clear session |
| GET | `/auth/status` | Get current user info |
| POST | `/dev/login` | Dev mode role selection |

### Dev Mode

For local development without OIDC:

```bash
# After setting AUTH_MODE=dev
curl -X POST http://localhost:8080/api/v1/dev/login \
  -H "Content-Type: application/json" \
  -d '{"role": "admin"}'
```

Returns a session cookie.

---

## Authorization (RBAC)

### Roles

| Role | What They Can Do |
|------|------------------|
| **Viewer** | Read-only access to everything |
| **Operator** | Deploy, rollback, sync flakes, manage systems |
| **Admin** | All of above + user management, audit log |

### Authorization Middleware

Every protected endpoint uses middleware to check permissions:

```rust
// Example: Operator or Admin only
async fn handler(
    State(state): State<AppState>,
    Session(user): Session,  // Gets current user from cookie
) -> Result<Json<...>, Error> {
    // Check role
    if user.role == "viewer" {
        return Err(Error::forbidden("Viewers cannot do this"));
    }
    // ... handler logic
}
```

### Environment Scoping

Users can only see **systems in their assigned environments**.

```sql
-- Query includes WHERE environment_id IN (user's environments)
SELECT * FROM systems 
WHERE environment_id IN (
  SELECT environment_id 
  FROM user_environment_memberships 
  WHERE user_id = ?
);
```

**Exception:** Admins can see all systems regardless of environment.

Snapshot APIs preserve non-disclosure. An unknown resource, a resource in a
hidden environment, and a revision outside the resource's active source use the
same not-found response. See [Evaluation and Flake Snapshot
Architecture](../evaluation-flake-snapshots.md#flake-outputs-and-count-authority).

---

## Systems API

Systems are the NixOS machines CF manages.

### Endpoints

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

### Compliance bundle assignment scope

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

`GET /compliance/bundles/:id/systems` without `version_id` remains a
single-version convenience alias for the bundle's current published (or draft)
version. It does not combine systems pinned to other versions. The exact
`?version_id=<uuid>` form lists only systems whose effective assignment targets
that version. The catalog `applicable_system_count` keeps its current-version
unit, consistent with the unversioned bundle-systems alias. A lineage-wide
assigned-system count would need a separately named contract.

### Query Parameters

```bash
# Filter by environment
GET /api/v1/systems?environment=prod

# Filter by status
GET /api/v1/systems?status=online

# Search by name
GET /api/v1/systems?search=web
```

### Example: List Systems

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

### Example: Trigger Deployment

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

---

## Flakes API

Flakes are git repositories that contain NixOS configurations.

### Endpoints

| Method | Endpoint | Role | Description |
|--------|----------|------|-------------|
| GET | `/flakes` | Viewer+ | List registered flakes |
| POST | `/flakes` | Operator+ | Add flake to registry |
| GET | `/flakes/:id` | Viewer+ | Get flake details |
| PATCH | `/flakes/:id` | Operator+ | Update flake |
| DELETE | `/flakes/:id` | Operator+ | Remove from registry |
| POST | `/flakes/:id/sync` | Operator+ | Trigger git sync |
| GET | `/flakes/:id/commits` | Viewer+ | Get commit timeline |
| GET | `/flakes/:id/revisions/:revision/outputs` | Viewer+ | Read cached revision outputs |
| GET | `/flakes/:id/revisions/:revision/modules/:module/declarations` | Viewer+ | Read cached exported-module declarations |

### Example: Get Commit Timeline

**Request:**
```bash
GET /api/v1/flakes/flake-456/commits
```

**Response:**
```json
{
  "data": [
    {
      "sha": "abc1234def5678",
      "sha_short": "abc1234",
      "message": "Update nginx config",
      "author": "john@example.com",
      "date": "2024-01-15T10:00:00Z",
      "changed_files": 2
    }
  ]
}
```

## Evaluation and Flake Snapshot API

These endpoints read persisted snapshots only. GET requests do not evaluate
Nix, inspect Git, fetch repositories, enqueue work, or perform per-host work.
All `revision` values are complete 40- or 64-character hexadecimal SHAs.

### GET `/systems/:id/evaluated-options`

Query parameters:

| Parameter | Contract |
| --- | --- |
| `revision` | Required full SHA in commit mode. |
| `mode` | `commit` or `generation`; defaults to `commit`. |
| `generation` | Required retained generation number in generation mode. |
| `search` | Case-insensitive redacted search text; truncated to 256 characters. |
| `filter` | `all`, `overridden`, or `changed`. |
| `limit` | Clamped to 1-100; defaults to 50. |
| `offset` | Clamped to 0-100,000; defaults to 0. |
| `snapshot_token` | Optional on offset 0; required and a 64-character hexadecimal digest when `offset` is greater than 0. |

The response lifecycle is `queued`, `running`, `failed`, `available`, or
`unavailable`. `counts` is revision-global and independent of search/filter.
Every response includes `option_inventory_state`, which is `complete`,
`partial`, or `unavailable`, and bounded `option_inventory_diagnostics`. Each
partial diagnostic contains redacted `path_components`, a stable `code`, and a
redacted `message`. The server canonicalizes and deduplicates path components
after redaction. `option_inventory_diagnostics_truncated` is true when the
128-entry bound or redaction collisions omit diagnostic detail. Traversal
continues after the detail budget is full. A partial available response contains
only options observed outside unreadable prefixes. Its counts, total, and module
totals describe that observed corpus.
Commit mode selects only schema-V2 Config Inspector artifacts through
`config_snapshot_selections`. It does not fall back to a schema-V1 commit
artifact. Generation mode retains schema-V1 selection through the exact retained
generation identity.
Generation-mode Config validity is independent of rollback lineage. A complete
pre-0248 retained artifact remains readable after migration even though its
unverified deployment/store lineage makes rollback ineligible.
`total` is the number of rows for the active search/filter. Changed data and
`counts.changed` are absent when the selected inventory is partial or when no
valid first-parent or preceding retained generation snapshot exists. A
`changed` filter over a partial inventory returns no rows. Drift and other
selected-versus-baseline facts are unavailable for a partial inventory.
`module_count` is the exact count of distinct
`(source_input, source_revision, source_path)` tuples after redaction and
per-option bounding; it is not derived from the bounded option page.
An available response includes an opaque `snapshot_token`. In commit mode, the
token binds the selected and first-parent V2 artifacts, first-parent state, and
the selected and first-parent flake-output digests used for tracked provenance.
It also binds inventory completeness, retained diagnostics, and the certified
truncation state.
In generation mode, the token binds the exact selected artifact, retained
identity, and comparison baseline identity. Generation responses also return
`baseline_generation` when comparison is available. Continuations
send page one's token. A replaced selected artifact, replaced baseline, or
removed retained identity returns HTTP 409 `snapshot_changed`; counts, total,
rows, baseline, and provenance are read from one read-only `REPEATABLE READ`
transaction.
A request revalidates the system-local selected generation or exact commit and
selects its mode-specific first-parent V2 or nearest preceding usable-generation
V1 baseline inside
that transaction. It requires the immutable integrity marker computed by
recursive full-artifact validation before publication, then decodes only the
bounded page. Malformed content outside the requested search, offset, or limit
prevents certification and returns lifecycle `unavailable`, zero counts and
total, no token, and no rows. The response limit remains 100 rows.
The scalar safe-value variant accepts JSON strings, numbers, Booleans, and null.
It rejects arrays and objects; collections require their declared structured
variant.
A supplied token also returns `snapshot_changed` when the replacement is
failed, unavailable, or absent. The endpoint does not return replacement
lifecycle data before it rejects the stale token.

### GET `/systems/:id/evaluation-summary`

This endpoint uses the same mode-specific `revision`, `mode`, and `generation` selection and
non-disclosing system authorization as evaluated-options. The response is
scalar. It does not contain module-source or definition rows.
Unverified retained generation lineage does not affect a valid Config summary;
it affects rollback eligibility only.
The optional `snapshot_token` query parameter binds the summary to an artifact
selected by another Config response. An available response returns the same
token. The token also binds the exact comparison baseline. Generation responses
return `baseline_generation` when comparison is available. A stale token or
replaced selected/baseline identity returns HTTP 409 `snapshot_changed`.
A supplied stale token takes precedence over failed, unavailable, or absent
replacement lifecycle responses.
Snapshot integrity, derivation facts, latest state, and
seven-day observations use one read-only `REPEATABLE READ` transaction.

The response returns lifecycle, safe error, persisted completion time,
evaluation duration, option total, `module_source_total`, exact selected NixOS
toplevel store path, existing closure package count, exact latest running store
path, agent-reported profile match, and drift. `module_source_total` is the exact
count of distinct `(source_input, source_revision, source_path)` tuples after
redaction and per-option bounding. Response-only tracked identities do not
affect the count. Drift is `matches` only when selected and running store paths
are exactly equal, `differs` only when both paths exist and differ, and
`unavailable` otherwise. A partial option inventory always reports drift and
comparison-derived summary facts as unavailable. Scalar facts that do not
require a complete inventory remain available.

In generation mode, `host_delta_count` is materialized from the schema-V1 usable
configuration snapshots at the selected commit. For each option path, the server
selects the most frequent complete safe content digest, including definition
provenance; missing is also a state, and bytewise state identity breaks ties. The
count is the selected snapshot's differences from that modal corpus. A usable
one-configuration corpus returns zero. Commit-mode V2 snapshots remain outside
that corpus and return null. Null otherwise means no usable materialized result
exists.

`closure_size_bytes` is the sum of `narSize` for every unique store path from
one successful complete recursive Nix query of the selected toplevel output.
Null means no complete local measurement was persisted. The server does not
substitute derivation size, snapshot size, or a partial query.

`agent_fingerprint` compares the exact selected and latest agent-reported store
paths. It is `matches`, `differs`, or `unavailable` when either path is absent.
`seven_day_drift` is `no_observed_drift` or `observed_drift` only when persisted
state and heartbeat observations span the full trailing seven days, every
boundary or adjacent gap is at most four hours, and all observations have an
exact store path. The observation before the window establishes coverage but
does not contribute drift. Otherwise it is `insufficient_coverage`.

Completion time, duration, selected and running paths, closure counts, profile
match, and other optional facts are null when their named persisted source is
absent. A non-available lifecycle returns no summary facts and zero totals.
Clients MUST render unavailable states. They MUST NOT infer one metric from
another field or replace null, unavailable, failed, or insufficient coverage
with zero or success.

### GET `/systems/:id/evaluation-module-sources`

This endpoint uses the same selected-revision and non-disclosure contract as
evaluated-options.

| Parameter | Contract |
| --- | --- |
| `revision` | Required full SHA in commit mode. |
| `mode` | `commit` or `generation`; defaults to `commit`. |
| `generation` | Required retained generation number in generation mode. |
| `limit` | Clamped to 1-100; defaults to 50. |
| `offset` | Clamped to 0-100,000; defaults to 0. |
| `snapshot_token` | Optional on offset 0; required and a 64-character hexadecimal digest when `offset` is greater than 0. |

The response lifecycle is `queued`, `running`, `failed`, `available`, or
`unavailable`. Non-available responses contain no token or rows and return a
zero total. An available response returns one bounded page and a
snapshot-version token. `total` is the exact complete-snapshot tuple count even
when `sources` is empty because the offset is past the final row.
Unverified retained generation lineage does not affect module-source reads from
a valid artifact; it affects rollback eligibility only.

Rows are ordered by `won_count` descending, `defined_count` descending, then
`source_input`, `source_revision`, and `source_path` in ascending bytewise
order. Null input and revision values sort last. Each row contains the exact
tuple, snapshot-wide counts for that tuple, and optional server-issued
`tracked_flake` identity.

The first request omits `snapshot_token`. Every continuation request sends the
token from the first page. If the persisted snapshot is replaced, the endpoint
returns HTTP 409 with `snapshot_changed` and no rows. The client discards all
loaded rows and restarts at offset 0.

The module-source token uses the same selected-and-baseline identity as the
options and summary endpoints. A baseline replacement therefore also returns
HTTP 409 instead of mixing Config data from different comparisons.
Failed, unavailable, and absent replacements use the same precedence when the
request supplies a stale token.

### GET `/systems/:id/generations`

Each generation row includes `generation_snapshot_id` and `rollback_eligible`.
Eligibility is true only when the retained row resolves an available immutable
artifact, exact derivation lineage, and non-empty server-side source store path.
Legacy retained rows with unverifiable deployment/store lineage remain
queryable but are not rollback-eligible.
Clients MUST NOT advertise rollback for an ineligible row.

### POST `/systems/:id/rollback-generation`

The request MUST contain `generation_snapshot_id` or the system-local
`generation`. `store_path` is optional and, when present, only narrows the
retained lookup. A store path alone never authorizes rollback. The server carries
the retained derivation identity into
composite authorization; a newer derivation with the same store path cannot
replace it. Foreign retained
identities, failed artifacts, and mismatched artifact/derivation lineage fail
closed.

`tracked_flake` is response-only and is never persisted in evaluator content.
For `self`, the source revision must equal the page's exact active context
revision. For an external input, the context revision's persisted lock snapshot
must match the exact input name, repository URL, and full locked revision. The
identity is returned only when this mapping resolves unambiguously to one
non-deleted registered flake and non-archived commit visible through an active
managed system. Hidden, stale, unmatched, deleted, archived, and ambiguous
identities remain absent. Repository URLs are sanitized before serialization.

The same response-only resolver decorates every selected and baseline
definition returned by `/evaluated-options`, using the selected or baseline
revision as that definition's context. The browser independently loads summary,
module-source, and option pages. It MUST NOT infer identities or derive a
snapshot-wide module count from a bounded page.

This GET is database-only. It does not evaluate Nix, inspect Git, fetch a
repository, enqueue work, mutate snapshot state, or perform per-host work.

### POST `/systems/:id/config-inspections/:revision`

This mutation requires administrator authority and matching CSRF credentials.
Authorization and environment visibility checks occur before revision
validation or resolution. The server atomically resolves the system's exact
active flake commit, effective configuration name, completed NixOS derivation,
and non-empty carrier `.drv` path. It then queues or reuses only the exact
Config Inspector target. A queued or running job is reused, and terminal history
permits a retry. An available complete V2 artifact suppresses work only when it
is comparison-ready. A certified partial V2 artifact also suppresses work
because retrying cannot make its observed corpus more complete without a source
change. A complete artifact with unavailable global provenance or Stage 2 does
not suppress a retry. In both reusable states, the carrier path must match
exactly. The enqueue decision acquires the
snapshot-writer transaction lock before target row locks and readiness checks.
If active work has a different derivation ID or carrier path, the endpoint
returns retryable HTTP 409 with `error: config_inspection_target_conflict` and
does not mutate that work.

If the exact completed carrier is absent, the endpoint returns HTTP 409 with
`error: config_inspection_prerequisite`. This response does not queue primary
evaluation. The endpoint does not change commit evaluation status or attempts,
notify primary evaluator or build queues, invoke Nix, or inspect another
configuration. Unknown systems and revisions outside the system's flake return
the same non-disclosing not-found response.

### POST `/systems/:id/evaluations/:revision` (explicit prerequisite)

This mutation requires administrator authority because the evaluator processes
the complete commit, and it requires matching CSRF credentials. It queues a
missing terminal evaluation or reuses
available, queued, or running work. The `queued` response field is true only
when this request performed the queue transition. The System Config UI does not
call this route. A caller uses it only as an explicitly named whole-commit
prerequisite when the targeted Config inspection route reports a missing
carrier. Completion does not guarantee carrier reconstruction: the primary
evaluator must discover and persist the exact successful NixOS target.

### GET `/flakes/:id/revisions/:revision/outputs`

Query parameters:

| Parameter | Contract |
| --- | --- |
| `system_filter` | `all`, `declared_unmanaged`, or `managed_undeclared`; defaults to `all`. |
| `limit` | Clamped to 1-100; applies independently to each top-level collection and to filtered reconciliation. |
| `offset` | Clamped to 0-100,000; applies independently to each top-level collection and to filtered reconciliation. |
| `snapshot_token` | Optional opaque token returned by the endpoint. When supplied, it binds the request to the selected output and usable first-parent comparison state. |

The server applies `system_filter` before the reconciliation offset and limit.
`pagination.system_total` is the visible total for the active filter, and
`pagination.systems_has_more` reports whether that filtered sequence has a next
row. The aggregate reconciliation counts, collapse count, pinned count, and
stale-input count remain revision-global. Clients request continuation pages
and retain these authoritative totals. A response larger than the 2 MiB safe
response bound is `unavailable` rather than silently truncated.

Token-aware clients send the first page's `snapshot_token` on continuation
requests. The server returns `409 snapshot_changed` if a supplied token is
stale or malformed because the selected output, first-parent identity or state,
or usable first-parent output changed. The client then discards accumulated
rows and restarts at offset 0. For compatibility with existing clients, a
positive offset without `snapshot_token` retains the prior bounded offset
semantics and does not receive this replacement guarantee. HTTP 409 applies
only when the request supplied a stale or malformed token.

`managed_system_count` is the authoritative visible active fleet count. It can
exceed the bounded `systems` array. Non-admin responses remove hidden systems,
configuration names, and module consumers. A caller without a visible active
managed system for the flake receives not-found.

Exported-module entries in this response are summaries. `declaration_count`
remains authoritative. `declarations` is empty, and `declarations_complete` is
false when declaration details exist. Clients use the dedicated declaration
endpoint instead of treating the summary as a complete nested collection.

An exported module's `source_input`, `source_revision`, and `source_path`
describe only the location of its `nixosModules` attribute binding. The
evaluator uses the Nix attribute position and requires one unambiguous longest
matching input root; `source_path` is relative to that root. Missing positions
and ambiguous roots produce null. These fields are not module value provenance
and do not authorize navigation. Declaration `source_paths` are the declaration
locations.

Input rows expose `direct_descendant_count` for immediate lock-graph children
and `transitive_descendant_count` for all unique recursive descendants of a
direct root input. Both counts use the complete lock graph, not the response
page. They are null for non-direct nodes or unavailable counts. Clients that
describe transitive reach MUST use `transitive_descendant_count`.

### GET `/flakes/:id/revisions/:revision/modules/:module/declarations`

This endpoint returns declarations for one exact exported module from one
persisted flake-output JSONB snapshot. `limit` is clamped to 1-100 and `offset`
to 0-100,000. The response contains the authoritative `total`, applied
`offset` and `limit`, deterministic declaration rows, explicit snapshot
`lifecycle` and safe `error`, and a content-digest `snapshot_token`.

The first request omits `snapshot_token`. Every continuation request sends the
token returned by page one. If re-evaluation replaces the selected snapshot,
the endpoint returns `409 snapshot_changed`. The client must discard loaded
rows and restart at offset 0. Unknown active revisions and module names return
not-found. Unauthorized or hidden flakes use the same non-disclosing behavior
as the top-level output endpoint. The query is database-only and does not
mutate evaluation or snapshot state.

See [Evaluation and Flake Snapshot
Architecture](../evaluation-flake-snapshots.md) for extraction ownership,
identity, comparison, persistence, retention, redaction, and verification
requirements.

---

## Builders API

Builders are worker processes that build Nix derivations.

### Endpoints

| Method | Endpoint | Role | Description |
|--------|----------|------|-------------|
| GET | `/builders` | Viewer+ | List builders |
| POST | `/builders` | Admin+ | Register builder |
| GET | `/builders/:id` | Viewer+ | Get builder details |
| PATCH | `/builders/:id` | Admin+ | Update builder |
| DELETE | `/builders/:id` | Admin+ | Remove builder |
| POST | `/builders/:id/pause` | Admin+ | Pause builder |
| POST | `/builders/:id/resume` | Admin+ | Resume builder |
| GET | `/builders/:id/jobs` | Viewer+ | Builder's job history |

### Builder States

| State | Meaning |
|-------|---------|
| idle | Waiting for work |
| building | Currently building |
| paused | Admin paused |

---

## Evaluation Queue API

The evaluation queue manages commit evaluations (nix-eval-jobs runs).

### Endpoints

| Method | Endpoint | Role | Description |
|--------|----------|------|-------------|
| GET | `/commits/eval-queue` | Viewer+ | Get evaluation queue with status |
| POST | `/commits/eval-queue/reorder` | Operator+ | Change queue order |

### GET /commits/eval-queue

**Response:**
```json
{
  "active_queue": [
    {
      "commit_id": 123,
      "flake_id": 1,
      "flake_name": "nixos-configs",
      "git_commit_hash": "abc123...",
      "commit_message": "Update system configs",
      "commit_timestamp": "2024-03-02T12:00:00Z",
      "evaluation_status": "in_progress",
      "eval_queue_position": 1,
      "system_statuses": [
        {
          "system_name": "nixos-desktop",
          "status": "evaluating"
        },
        {
          "system_name": "nixos-server",
          "status": "policy_passed"
        }
      ]
    }
  ],
  "completed_queue": [...]
}
```

### POST /commits/eval-queue/reorder

**Request:**
```json
{
  "commit_id": 123,
  "new_position": 2
}
```

Moves the specified commit to the given position in the queue. Queue positions are recalculated for all affected commits.

### Evaluation States

```
pending → in_progress → complete
            ↓
          failed
```

**Per-System States** (during in_progress):
```
pending → evaluating → eval_complete → policy_check
                 ↓              ↓
            eval_failed    policy_passed / policy_failed
```

**Key Invariant:** Only ONE commit can have `evaluation_status = 'in_progress'` at a time.

---

## Build Queue API

The build queue manages Nix derivation builds.

### Endpoints

| Method | Endpoint | Role | Description |
|--------|----------|------|-------------|
| GET | `/build-jobs` | Viewer+ | Get a bounded page of pending/in-progress builds |
| GET | `/build-jobs/recent` | Viewer+ | Get a bounded page of terminal build attempts |
| GET | `/build-jobs/:id` | Viewer+ | Get one exact visible attempt and its active or completed collection |
| POST | `/build-queue` | Operator+ | Queue new derivation |
| DELETE | `/build-queue/:id` | Operator+ | Cancel pending build |

The exact build-attempt endpoint applies the caller's environment visibility in
the primary-key query. It returns `404 Not Found` for both missing attempts and
attempts outside the caller's visibility scope. This behavior prevents attempt
identity disclosure. Exact lookup does not expand either paginated list and does
not treat a UUID as ordinary text search.

### Build States

```
pending → building → built → cache-pushing → cache-pushed
           ↓            ↓           ↓
         failed    cache-failed  cache-failed
```

---

## Environments API

Environments group systems logically.

### Endpoints

| Method | Endpoint | Role | Description |
|--------|----------|------|-------------|
| GET | `/environments` | Viewer+ | List environments |
| POST | `/environments` | Admin+ | Create environment |
| GET | `/environments/:id` | Viewer+ | Get environment |
| PATCH | `/environments/:id` | Admin+ | Update environment |
| DELETE | `/environments/:id` | Admin+ | Delete environment |

---

## Dashboard API

Aggregated fleet data.

### Endpoints

| Method | Endpoint | Role | Description |
|--------|----------|------|-------------|
| GET | `/dashboard` | Viewer+ | Fleet summary |
| GET | `/dashboard/builds` | Viewer+ | Build queue summary |
| GET | `/dashboard/flakes` | Viewer+ | Flake sync status |

### Example Response

```json
{
  "data": {
    "systems": {
      "total": 10,
      "online": 8,
      "offline": 2
    },
    "environments": {
      "production": 5,
      "staging": 3,
      "development": 2
    },
    "builds": {
      "pending": 3,
      "building": 1,
      "recent": [...]
    }
  }
}
```

---

## Admin API

Admin-only endpoints for user and system management.

### Users Management

| Method | Endpoint | Role | Description |
|--------|----------|------|-------------|
| GET | `/admin/users` | Admin+ | List users |
| POST | `/admin/users` | Admin+ | Create user |
| GET | `/admin/users/:id` | Admin+ | Get user |
| PATCH | `/admin/users/:id` | Admin+ | Update user |
| DELETE | `/admin/users/:id` | Admin+ | Delete user |

### Audit Log

| Method | Endpoint | Role | Description |
|--------|----------|------|-------------|
| GET | `/admin/audit` | Admin+ | List audit events |
| GET | `/admin/audit/export` | Admin+ | Export audit log |

**Query Parameters:**
```bash
GET /api/v1/admin/audit?start_date=2024-01-01&end_date=2024-01-31&actor=john
```

### OIDC Mappings

| Method | Endpoint | Role | Description |
|--------|----------|------|-------------|
| GET | `/admin/oidc-mappings` | Admin+ | List mappings |
| POST | `/admin/oidc-mappings` | Admin+ | Create mapping |
| PATCH | `/admin/oidc-mappings/:id` | Admin+ | Update mapping |
| DELETE | `/admin/oidc-mappings/:id` | Admin+ | Delete mapping |

---

## CVE Scan Operations

### Scan Diagnostics

| Method | Endpoint | Role | Description |
|--------|----------|------|-------------|
| GET | `/scanning/scans` | Admin | Return one bounded Active, Completed, or History scan-record page |
| GET | `/scanning/scans/:scan_id` | Admin | Return bounded diagnostics for one exact CVE scan |

The collection route accepts `collection=active|completed|history` and a `limit`
from 1 through 500. `history` requires `system_id` and returns that system's
bounded revision history without a continuation cursor. Completed requests
also accept normalized `q` (with `search` retained as an alias), terminal
`status`, revision class, latest-per-flake, archive visibility, sort, direction,
and an opaque `after` cursor. The server applies those values to the complete
collection before counting or paging. Completed rows use deterministic keyset
order with terminal timestamp and scan ID tie-breakers. Responses include
`total`, `hidden_archived`, `has_more`, and `next_cursor`.

The versioned cursor binds every normalized request value and the first page's
terminal high-water tuple. A request with changed filters or ordering must start
without a cursor. Malformed cursors return 400. A cursor rebound to another
request returns 400. Newer terminal rows cannot enter continuation pages from an
existing walk. Archive and restore operations continue to use exact scan IDs;
including archived rows does not change chronological keyset semantics.

The endpoint returns `scan_id`, current `status`, `scanner_name`, optional
`scanner_version`, `source_trigger`, an `events` array, and `truncated`. Each
event contains an immutable row `id`, immutable `execution_id`, and one-based
`attempt_number` identity, `occurred_at`, normalized `level`, `source`,
`event_type`, redacted `message`, and an event-level `truncated` flag.

The response uses a fixed limit of 500 events in attempt and server receipt
order. The builder-supplied `occurred_at` value is informational and cannot
reorder lifecycle events. `truncated=true` means later persisted events exist.
This endpoint does not provide cursor or offset pagination. Clients must not
infer that a truncated response contains the complete attempt history. Unknown
scan IDs return `404`. Non-admin callers receive the standard admin authorization
failure.

Diagnostic messages are untrusted operational data. Builders can omit the
optional diagnostics field for backward compatibility. The server accepts at
most 256 prepared events per terminal report, persists at most 2,048 Unicode
scalar values per event, removes control characters, and applies canonical
secret redaction before the first database write. Upgraded builders also apply
their shared credential-redaction policy before request serialization.
The heartbeat endpoint additionally accepts up to 16 single-line phase events
in a 64 KiB body. It renews the fenced lease and appends those events atomically,
and it deduplicates uncertain retries by execution and event type. Diagnostics
are independent from canonical CVE evidence and are not included in the schema-1
evidence digest.

---

## Fleet CVE Triage

Fleet triage uses exact deployed evidence. The identity is a canonical CVE ID
plus a canonical package name. Package version is evidence context and is not
part of the stable finding identity.
The [CVE/POA&M continuity contract](../design/CrystalForge/cve-poam-evidence-continuity-design-spec.md#29-acceptance-criteria)
governs Current CVE authority, baseline continuity, verification, and environment
membership. TASK-326.2.2 is implementing this contract; this section does not
assert that all paths are deployed or verified. Config inspection and rollback
keep their separate retained-artifact authority.

### System CVE Inventory

| Method | Endpoint | Role | Description |
|--------|----------|------|-------------|
| GET | `/systems/:id/cves` | Viewer+ | Return the compatible bare array of current exact findings |
| GET | `/systems/:id/cve-inventory` | Viewer+ | Return the complete compatibility inventory up to 1,000 rows |
| GET | `/systems/:id/cve-inventory-page` | Viewer+ | Return a bounded exact, read-only mapped-running, historical, or no-scan inventory page |

The paged inventory response contains `authority`, `exact_authority_failure`,
`current_state`, `running_target`, `system_id`, `selection`, `attempt`,
`evidence_representation`, `read_only`, `source`,
`vulnerabilities`, `metadata`, `has_more`, `inventory_revision`, and
`next_cursor`. `authority` is `exact`, `mapped_running`, `legacy`, or `no_scan`.
`source` contains
the real scan ID, scanner name and optional version, and completion time.
`attempt` is optional and records the newest scan ID, derivation ID, persisted
status, and creation time for the selected exact derivation. It is ordered by
`COALESCE(created_at, scheduled_at) DESC NULLS LAST, id DESC` in the same read
transaction. The newest attempt is independent of `source`: a newer pending,
in-progress, or failed scan does not replace completed evidence or authorize
triage. An unmapped Current selection has neither an exact attempt nor a source.
The attempt does not change inventory pagination or the source revision. The
client shows a "newer attempt" notice only when the attempt's creation time is
after the selected source's completion time. An older or undated attempt does
not claim to be newer.

For `selection=current`, the server selects the latest reported state before
checking its validity. It counts every derivation whose output matches that
report in the registered flake and effective configuration; the bounded
candidate menu does not prove uniqueness. A unique mapping selects that exact
derivation's newest completed schema-1 scan by `completed_at DESC, id DESC`.
For CVE-domain Current authority, the latest observation must have a generation,
a usable store path, and true generation/store agreement. The mapping must
identify exactly one NixOS derivation in the registered flake and effective
configuration. Its newest completed schema-1 scan is the exact source. The
shared `view_current_cve_authority` is the target authority for inventory,
fleet/triage, link, and verification paths. Retained evaluation-generation
provenance is optional; a missing or unavailable Config artifact does not make
this exact CVE source read-only. The origin of activation and distance from
flake head do not change CVE authority. Server-owned external retention can
still supply extra provenance; it is not a prerequisite for CVE mutation and
must not fabricate a CF deployment or evaluation artifact.

`mapped_running_read_only_scan` is a compatibility display state for a uniquely
mapped scan that does not qualify as exact Current CVE authority; it is not a
permanent state solely because retained proof is missing. A unique target without
an eligible scan remains `mapped_running_no_scan` or `no_current_scan` as
applicable. `no_running_report`, `invalid_running_report`, `unmapped_running`,
and `ambiguous_running` remain source-less `no_scan` states.
`system_id` is returned on paged responses and `running_target` contains only
the unique derivation ID, registered commit hash, trusted reported generation
when bound to the output, and report time. An absent source never claims clean;
a completed scan with zero eligible findings retains its source and totals.
Unsupported enum values make older clients fail to parse conservatively instead
of treating mapped-running evidence as fully authorized `exact` evidence.

Historical `retained_generation` and `exact_derivation` selections remain
exactly bound and read-only. Their evidence can be schema-1 observations or a
schema-0 historical projection, but is never promoted to Current authority.
Clients select them with `target=retained_generation&target_id=<snapshot UUID>`
or `target=exact_derivation&target_id=<derivation integer>`. Current is the
default and can be requested explicitly with `target=current`; it has no
`target_id`.
Sources are never unioned. Historical findings can include ordinary system
justification state, but never server-issued exact remediation context. The
mapped-running read-only tier also omits that context. POA&M creation, patch
scheduling, finding attach/link/reopen, verification, and closure re-resolve
latest-first exact Current CVE authority on the server under writer locks.
Missing or inconsistent latest state, ambiguous or foreign mapping, missing
completed schema-1 scan, wrong-derivation or historical evidence, and unauthorized
scope fail closed. No source is not a clean scan. Inventory GET requests run in
read-only repeatable-read transactions and do not repair retained proof, enqueue
scans, run Nix, or persist deployments.

The paged route accepts `limit` from 1 through 500 with a default of 100, an opaque
`after` cursor, `q` up to 200 normalized characters, comma-separated `severity`
values (`critical`, `high`, `medium`, `low`, or `unknown`), and comma-separated
fix-availability `status` values (`open` or `fix_available`). Status does not
represent triage state. SQL applies filters before full-scope metadata and page
selection. Severity metadata includes the active severity filter.

Rows use C-collated keyset order by canonical CVE ID and canonical package
name. Each row exposes both values as its stable identity. The versioned cursor
binds the system, authority, scan, normalized filters, inventory revision, and
last identity. The revision covers the selected source and every mutable field
that affects stable identity, search, severity and fix-availability filters,
order, or totals. Description, CVSS changes within one severity, justification,
and exact remediation state do not invalidate membership pagination. The server
hydrates those display and remediation fields from current authorized state for
only the returned rows. The cursor position is unsigned and non-authoritative;
tampering can only skip rows within an inventory that the caller can already
read.
Malformed cursors return 400 after system authorization. Source, system, or
filter mismatch returns `inventory_changed` with status 409. Hidden and absent
systems return the same 404 before cursor validation. Requests without query
parameters receive the bounded first page; clients that need the full
inventory must follow `next_cursor`. Exact remediation context is loaded only
for exact rows in the returned page. Legacy rows never receive it.

The compatibility route keeps the original DTO and severity-first order. It
returns the complete selected inventory when there are at most 1,000 stable
rows and returns HTTP 400 above that bound. Rolling deployments can therefore
serve old clients from the compatibility route while the current Web UI uses
the paged route. Unknown or unrecognized source severity serializes as `low` on
the compatibility route; the paged route preserves the explicit `unknown`
value.

### Exact-CVE POA&M Routes

| Method | Endpoint | Role | Description |
|--------|----------|------|-------------|
| POST | `/poams/cves` | Operator+ | Create a POA&M from one server-issued exact occurrence |
| GET | `/poams/relationships/cves?system_id=:id` | Viewer+ | Return bounded current exact occurrences and POA&M relationships |
| POST | `/poams/:id/cve-findings` | Operator+ | Link one current exact occurrence |
| DELETE | `/poams/:id/cve-findings/:finding_id?revision=:revision` | Operator+ | Retire one exact finding link |
| POST | `/poams/:id/verify` | Operator+ | Seal exact current verification evidence |
| POST | `/poams/:id/close` | Operator+ | Verify and close atomically |
| POST | `/poams/:id/reopen` | Operator+ | Restore the exact closure finding set |

All routes require an authenticated session. Mutation routes require matching
CSRF cookie and header values. Operator and Admin roles can mutate. Viewer can
read visible relationships but cannot mutate. A typed POA&M assignee does not
grant environment access or mutation authority. Unknown resources and resources
outside the caller's environment scope return the same `404` response. Create,
link, and system-CVE justification transactions re-read the caller's active-user
state, roles, and environment memberships after they acquire their writer
locks. Revocation during a concurrent request prevents mutation even when the
request-time actor snapshot was authorized.

The create and link bodies contain an opaque `observation` with `system_id`,
`scan_id`, `occurrence_derivation_path`, `canonical_cve_id`, and
`canonical_package_name`. The server re-resolves this context against the newest
completed schema-1 scan for the exact observed Current derivation.
Clients must not construct or modify this context. Create accepts at most 100
assignment-version references. Policy findings retain their 100-active-link
limit. Exact-CVE links can exceed 100 as current environment membership changes;
bounded read pages and verification-item batches do not truncate closure proof.
Relationship history defaults to 100 rows, accepts a limit from 1 through 100,
and returns no more than 1,000 current exact occurrence rows.

Exact-CVE unlink, verify, close, and reopen operations and fleet triage also
re-read the active user, roles, and memberships after their domain writer
locks. A concurrent role or membership revocation therefore cannot authorize a
waiting mutation.

Link, unlink, verify, close, reopen, update, and transition operations use the
current POA&M `revision`. A stale revision returns `409 stale_revision`. Exact
finding links retain an immutable server-resolved link-time baseline: scan,
derivation, completion time, observed generation, target store path, occurrence
derivation path, observed package version, and optional retained-generation ID.
Existing non-null retained IDs remain historical proof. The API does not accept
baseline fields from clients. Exact verification returns `pass` only when a
strictly newer completed schema-1 scan of the exact observed Current derivation
omits the canonical CVE/package occurrence. The baseline scan and scans completed
before it cannot pass verification. A changed commit, generation, derivation,
package version, or activation origin alone does not return `missing`. Present,
whitelisted, justified, legacy, historical, unavailable, or inconsistent evidence
does not pass. A clean newer scan makes remediation a candidate; it does not
close the POA&M. If both whitelisted and unwhitelisted paths for the same
canonical pair exist in that scan, the unwhitelisted path determines the
verification result. A rejected close records and
returns the committed verification attempt as `412 closure_not_ready`; clients
must continue with the returned committed revision.

POA&M detail and verification responses are rolling-compatible. `findings` and
`items` retain policy-finding meanings. New servers add `cve_findings` to POA&M
detail and `cve_items` to verification attempts and verify/close results. Older
clients must ignore these fields. New clients must default absent fields to an
empty array while servers are upgraded. Exact finding rows include stable
system/CVE/package identity and evidence context. Exact verification rows also
include the observed package version, scan, observed generation and optional
retained-generation provenance, result, and bounded diagnostic detail. Each
row distinguishes immutable baseline evidence from current verification
evidence. Both cited scans are
retained for audit while their finding or verification records exist.

POA&M detail returns active and retired exact finding links. Retired rows retain
their link-time scan, scan completion time, deployed generation, target store
path, occurrence path, observed package version, retirement time, and retirement
reason. Retired rows are immutable and cannot be unlinked again. Completed
POA&Ms retain these rows as their exact-vulnerability audit display.

Important exact-CVE errors include `invalid_cve_identity`,
`stale_cve_observation`, `cve_occurrence_whitelisted`,
`cve_occurrence_justified`, `finding_already_managed`, `incompatible_finding`,
`too_many_findings`, `finding_required`, `concurrent_finding_change`,
`invalid_transition`, `stale_revision`, and `closure_not_ready`. Validation
errors use HTTP 400, authorization uses 403, hidden or absent resources use 404,
stale/lifecycle conflicts use 409, and failed closure preconditions use 412.

| Method | Endpoint | Role | Description |
|--------|----------|------|-------------|
| GET | `/cves/:cve_id/fleet?package=:pname` | Viewer+ | Return visible affected environments and current dispositions |
| POST | `/cves/:cve_id/triage` | Operator+ | Apply environment actions atomically |

The GET response contains `cve`, `canonical_package_name`, `rollup`, total,
exact, legacy-affected, no-scan, and unassigned system counts, `environments`,
and bounded `unassigned_systems`. Each
environment contains its UUID, name, total, exact, and legacy-affected counts,
bounded system details, and an optional tagged `disposition`. Each system row
identifies `inventory_authority` as `exact` or `legacy`. A missing disposition
means OPEN only for that environment's exact subjects. A legacy-only environment is
inventory-only and cannot be triaged. `rollup` is `outstanding`, `accepted`,
`scheduled`, or `partial` and describes exact dispositions only. Admin-visible
unassigned systems are returned as inventory-only because fleet triage is
environment-scoped. The endpoint rejects more than 1,000 affected systems
instead of returning a partial drawer. The endpoint
returns `404` when no current exact or legacy finding is visible. It does not
reveal hidden environment names or counts.

A `scheduled` disposition always includes the referenced active POA&M as nested
`poam` metadata on a new server:

```json
{
  "state": "scheduled",
  "poam_id": "...",
  "poam": {
    "id": "...",
    "human_id": "POAM-0042",
    "title": "Remediate CVE-2026-12345",
    "plan": "Promote the fixed package through environments",
    "target_date": "2026-10-15",
    "risk": "high",
    "assignee": {
      "kind": "user",
      "user_id": "...",
      "display": "Fleet owner",
      "available": true
    }
  },
  "actor": {"user_id": "...", "display": "Scheduling operator"},
  "scheduled_at": "2026-09-13T12:00:00Z"
}
```

The nested value contains the exact title, plan, target date, risk, and typed
assignee required for semantic POA&M reuse. `id` is the stable UUID and
`human_id` is the stable operator-facing label. The assignee can be `user`,
`oidc_group`, `unassigned`, or `legacy`. For typed assignees, `available`
reports current catalog eligibility only. An unavailable or compatibility
assignee remains visible as historical ownership but cannot be selected for a
new fleet scheduling request. An assignee never grants authorization. Fleet
reads fail closed instead of returning SCHEDULED when the referenced POA&M is
completed or lacks compatible metadata.

The nested `poam` field is additive for rolling upgrades. New Web UI clients
accept an absent field from an old server. If scheduled rows omit it, refer to
different POA&Ms, or contain an assignee that cannot be reused, the editor shows
a non-destructive upgrade or ownership conflict and blocks submission while any
environment remains scheduled. The operator can still change all scheduled
environments to OPEN or ACCEPTED and submit when POA&M lifecycle rules permit.
When scheduled rows share one reusable POA&M, the editor initializes the shared
draft from this metadata so an unchanged scheduled row survives mixed edits.

The POST body contains one action for every currently visible environment that
has at least one Current exact subject:

```json
{
  "canonical_package_name": "openssl",
  "actions": [
    {"action": "accept_risk", "environment_id": "...", "justification": "...", "review_date": "2026-10-01"},
    {"action": "schedule_patch", "environment_id": "..."},
    {"action": "leave_open", "environment_id": "..."}
  ],
  "poam": {
    "title": "Remediate CVE-2026-12345",
    "plan": "Promote the fixed package through environments",
    "assignee": {"kind": "user", "user_id": "..."},
    "target_date": "2026-10-15",
    "risk": "high",
    "default_milestones": true
  }
}
```

`poam` is required exactly when at least one action is `schedule_patch`. The
assignee must be a server-validated user or OIDC group. Clients do not send host
IDs. After writer locks and a fresh actor-membership check, the server recomputes
the complete visible Current exact environment set. The request environment IDs
must equal that set. Scheduled-target-only and Historical environments do not
enter the action set. Omitted, extra, forged, hidden, or duplicate IDs return
the same typed evidence conflict without mutation and without identifying
hidden environments. The server includes every Current exact subject in each
actionable environment. All subjects selected for SCHEDULED use one POA&M.
ACCEPTED records operator rationale only; it does not create remediation links
or PASS evidence.

Authenticated CVE dashboard reads classify inventory as Current exact deployed
findings, exact active scheduled deployment targets, or retained Historical
evidence. An exact clean scan suppresses stale compatibility findings. Rows and
fleet statistics expose separate Current, Scheduled deployment target, and
Historical counts. Compatibility `affected_count` is the distinct union of
Current and Scheduled deployment target systems. Historical systems do not
contribute to that count. Fleet statistics also count visible active no-scan
systems.

Active dispositions apply only to Current exact canonical CVE, canonical
package, and environment identities. Scheduled-target-only and Historical-only
rows are `inventory_only`. Historical evidence on a row that also has Current
exact subjects does not remove mutation authority from those Current exact
subjects. Scheduled deployment intent does not imply the SCHEDULED triage state.
Legacy `system_cve_justifications` rows do not determine list status. Admin reads
cover the fleet. Viewer and Operator reads first limit subjects to current
`user_environment_memberships`. Scoped reads exclude unassigned systems and do
not disclose hidden environment names, counts, statuses, package names, CVE
presence, or fleet-wide justification rows. The legacy `GET /cves/:cve_id`
returns the alphabetically first visible canonical package row and returns `404`
when the CVE is absent or hidden. Lists return only visible rows.

A row is `accepted` only when all Current exact affected environments are
ACCEPTED. A row is `scheduled` only when all Current exact affected environments
are SCHEDULED. Any OPEN or mixed Current exact state is `outstanding`. A row
without Current exact subjects is `inventory_only`. Grouped counts, list
filters, export/list responses, and fleet statistics consume this conservative
summary. Package cards count distinct Current-or-Scheduled systems per package
after active filters. Fleet statistics count distinct systems across the scoped
Current-or-Scheduled union; they do not sum per-CVE counts. CVE totals count
canonical CVE/package inventory rows. The exact mutation rollup keeps its more
precise `partial`/MIXED state and exact-matches both the canonical CVE and
canonical package when it loads installed version, fixed version, and fix
status.

The successful response contains transaction-owned `detail`, `detail_scope`,
`poam_id`, and `poam_reused`; response construction completes before the
mutation commits. `detail_scope` is `exact_mutation_subjects`. The returned
`detail` excludes legacy and unassigned inventory rows. A client must refetch
the fleet inventory endpoint after success before it renders the drawer again.
Repeating an identical accepted-risk request does not retire and recreate its
disposition history. Repeating a schedule request reuses a compatible active
POA&M when all current affected environment-owned subjects are covered.
Historical clean or moved-out links may remain in the episode; they do not
count as current subjects or block reuse merely because the link set is larger.
Other actions in the same request may retire their own current ownership
atomically. Canonical CVE, package, domain, host-override precedence, and
semantic POA&M metadata must still match. Server-owned bounded reconciliation
adds newly affected subjects idempotently after relevant scan, state,
environment, and disposition changes, with periodic repair and post-lock
rechecks. It must not create duplicate active remediation, overwrite host
overrides, or treat missing evidence as clean.

An authorized A→B environment move retires A's environment-owned active CVE
link in the same transaction and preserves its immutable baseline as history.
An active A schedule may temporarily reference an open POA&M with no active
findings when its last member moved; later exact A subjects reuse that episode.
A direct host override stays with the system. B may schedule a distinct POA&M.
Non-admin A readers can inspect A's historical finding ID but cannot search
for the moved host's current B hostname or inspect its current B environment.
Reconciliation attaches at most 100 missing subjects per page and repeats until
coverage is complete. Verification writes bounded 100-item pages within one
transaction and never seals a partial subject set as successful closure.

Closing a fleet-created POA&M retires its active SCHEDULED dispositions with
its exact links. A later recurrence therefore reads as OPEN, not SCHEDULED by a
completed POA&M. Reopen restores SCHEDULED only when each environment's current
current affected owned subjects are covered by restored links and no active
disposition conflicts. Historical closure members need not remain affected.
Systems without an environment restore their exact links without an
environment disposition. Unlinking an environment's final active exact link
retires that environment's SCHEDULED disposition and does not change another
environment's disposition. Fleet reads suppress a SCHEDULED disposition when
its POA&M is completed or its current affected environment-owned subjects are
not covered by active links. Extra historical links do not invalidate current
coverage. Closure re-resolves current subjects; a completed POA&M cannot
silently reopen on recurrence.

The following conflict codes are significant:

| HTTP | Error | Meaning |
|------|-------|---------|
| 404 | `not_found` | An environment is unknown or outside the caller's scope |
| 409 | `environment_not_affected` | A selected environment has no current exact subject |
| 409 | `cve_evidence_changed` | Current affected systems changed while locks were acquired |
| 409 | `cve_subjects_already_managed` | Subjects are partially owned or POA&M metadata is incompatible |
| 409 | `cve_disposition_conflict` | Reopen cannot restore exact environment coverage |
| 409 | `poam_final_subject` | The action would leave an active POA&M without a finding |

Conflict responses use
`{"error":"code","message":"...","details":{...}}`. Subject conflict
details are bounded. Every conflict rolls back all requested actions.

---

## Agent API (Machine Auth)

These endpoints use **key-based authentication** (not user sessions). They're for builders and agents to communicate with the server.

### How It Works

1. Builder/Agent registers with a public key
2. Each request includes signature in header
3. Server verifies signature before processing

### Endpoints

| Method | Endpoint | Auth | Description |
|--------|----------|------|-------------|
| POST | `/agent/heartbeat` | Builder Key | Builder reports status |
| POST | `/agent/state` | Agent Key | Agent reports state |
| POST | `/agent/report` | Agent Key | Report build/deploy result |
| GET | `/agent/job` | Builder Key | Get next build job |
| POST | `/agent/job/:id/complete` | Builder Key | Report job complete |

### Example: Builder Gets Job

**Request:**
```bash
GET /api/v1/agent/job
X-Builder-Key: builder-key-id
X-Builder-Signature: signed-timestamp
```

**Response:**
```json
{
  "data": {
    "job_id": "job-123",
    "derivation": "nixosConfigurations.production.system.built",
    "store_path": "/nix/store/xxx-nixos-system-x86_64",
    "system": "sys-456"
  }
}
```

---

## Cache API (Future - TASK-141)

Binary cache management (not yet implemented).

| Method | Endpoint | Role | Description |
|--------|----------|------|-------------|
| GET | `/caches` | Admin+ | List caches |
| POST | `/caches` | Admin+ | Create cache |
| GET | `/caches/:id` | Admin+ | Get cache |
| PATCH | `/caches/:id` | Admin+ | Update cache |
| DELETE | `/caches/:id` | Admin+ | Delete cache |
| GET | `/environments/:id/cache-config` | Builder | Get cache for env |

---

## Common Error Codes

| Code | Meaning | When Used |
|------|---------|-----------|
| UNAUTHORIZED | No valid session | Not logged in |
| FORBIDDEN | Insufficient permissions | Logged in but wrong role |
| NOT_FOUND | Resource doesn't exist | ID is wrong |
| VALIDATION_ERROR | Invalid input | Bad request data |
| CONFLICT | Resource already exists | Duplicate create |

---

## Adding a New API Endpoint

### Step 1: Define the DTO

In `src/api/models.rs`:

```rust
#[derive(Serialize, Deserialize)]
pub struct NewWidget {
    pub name: String,
    pub description: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct WidgetResponse {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
}
```

### Step 2: Add Query (if database needed)

In `src/queries/widgets.rs`:

```rust
pub async fn create_widget(
    pool: &PgPool,
    data: NewWidget,
) -> Result<WidgetResponse> {
    let row = sqlx::query_as!(
        WidgetResponse,
        "INSERT INTO widgets (name, description) 
         VALUES ($1, $2) 
         RETURNING id, name, description, created_at",
        data.name,
        data.description
    )
    .fetch_one(pool)
    .await?;

    Ok(row)
}
```

### Step 3: Add Handler

In `src/handlers/api/widgets.rs`:

```rust
pub async fn create_widget(
    State(state): State<AppState>,
    Json(data): Json<NewWidget>,
    require_operator: RequireOperator,  // Middleware
) -> Result<Json<WidgetResponse>, Error> {
    let widget = queries::widgets::create_widget(&state.pool, data)
        .await
        .map_err(Error::from)?;

    Ok(Json(WidgetResponse { data: widget }))
}
```

### Step 4: Register Route

In `src/server/mod.rs`:

```rust
Router::new()
    .route("/api/v1/widgets", post(handlers::widgets::create_widget))
    // ... other routes
```

### Step 5: Add Authorization

```rust
// In server/mod.rs
.route(
    "/api/v1/widgets",
    post(handlers::widgets::create_widget)
        .layer(RequireOperator::new())  // Only Operator/Admin
)
```

---

## File Organization

```
src/
├── main.rs                 # Entry point
├── server/
│   └── mod.rs             # Route setup, middleware
├── handlers/
│   ├── mod.rs
│   ├── api/
│   │   ├── systems.rs
│   │   ├── flakes.rs
│   │   ├── builders.rs
│   │   ├── admin.rs
│   │   └── ...
│   └── agent/
│       ├── heartbeat.rs
│       └── ...
├── queries/
│   ├── mod.rs
│   ├── systems.rs
│   ├── flakes.rs
│   └── ...
├── models/
│   ├── mod.rs
│   ├── system.rs
│   └── ...
├── api/
│   └── models.rs          # DTOs (Data Transfer Objects)
├── config/
│   └── mod.rs             # Configuration
└── error.rs               # Error types
```

---

---

## WebSocket Streaming

### Evaluation Logs (Real-Time)

**Endpoint:** `ws://localhost:8080/ws/eval-stream/:commit_id`

**Purpose:** Stream evaluation logs in real-time as nix-eval-jobs runs.

**Protocol:**
1. Client connects with commit ID
2. Server checks if commit evaluation is in progress
3. If yes: streams log lines as they appear
4. If no: closes connection with "not found" message

**Message Format:**
```json
{
  "type": "log",
  "data": "evaluating system: nixos-desktop",
  "timestamp": "2024-03-02T12:34:56Z"
}
```

**System Status Updates:**
```json
{
  "type": "system_status",
  "system": "nixos-desktop",
  "status": "evaluating",
  "data": null
}
```

```json
{
  "type": "system_status",
  "system": "nixos-desktop",
  "status": "policy_passed",
  "data": {
    "queued_for_build": true
  }
}
```

**Status Values:**
- `pending` - Waiting to evaluate
- `evaluating` - Currently running nix-eval-jobs
- `eval_complete` - Evaluation succeeded
- `eval_failed` - Evaluation failed
- `policy_passed` - CF enabled, added to build queue
- `policy_failed` - CF disabled, skipped

**Key Files:**
- `src/handlers/websocket.rs` - WebSocket handler
- `src/models/evaluate_with_policies.rs` - Broadcasts status updates

---

## Summary

| Resource | Endpoints | Auth |
|----------|-----------|------|
| Systems | CRUD + deploy/rollback | Viewer+ |
| Flakes | CRUD + sync | Viewer+ |
| Builders | CRUD + pause/resume | Viewer+ |
| Build Queue | CRUD | Viewer+ |
| Eval Queue | GET + reorder | Viewer+ |
| Environments | CRUD | Viewer+ |
| Dashboard | GET | Viewer+ |
| Admin Users | CRUD | Admin+ |
| Admin Audit | GET | Admin+ |
| Admin OIDC | CRUD | Admin+ |
| Agent/Builder | Various | Key-based |
| WebSocket | eval-stream/:commit_id | Session |

For frontend views, see `01-frontend-views.md`.
For system overview, see `00-system-overview.md`.

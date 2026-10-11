---
type: API
title: "Builder API: job lifecycle endpoints"
description: "Documents the builder-signed endpoints for heartbeat, next-job polling (including 409 evaluator conflicts), derivation manifest and delta/full derivation archives, job completion, failure, and log append."
tags:
  - crystal-forge
  - builder
  - api
  - job
  - derivation-archive
implementation_status: implemented
generated:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-03T22:57:42-05:00
sources:
  - id: origin
    resource: "Crystal Forge repository file docs/multi-builder-api.md at commit 3b23d36f"
    title: "Multi-Builder API Documentation"
---

# Builder API: job lifecycle endpoints

## Builder Endpoints

All builder endpoints require builder signature authentication.

### POST /api/v1/builders/:id/heartbeat

Report builder heartbeat with resource metrics.

**Request Body**:
```json
{
  "cpu_usage_percent": 45.2,
  "memory_usage_mb": 2048,
  "system_cpu_usage_percent": 60.5,
  "system_memory_total_mb": 16384,
  "system_memory_used_mb": 8192
}
```

**Response**: `200 OK`
```json
{
  "status": "ok",
  "message": "Heartbeat recorded"
}
```

**Side Effects**:
- Updates `last_heartbeat_at` timestamp
- Marks a current-session builder as "active" if previously inactive or offline
- Persists scanner capability for a current enabled and registered session. An
  offline builder can persist capability before this heartbeat restores active
  state. Disabled, unregistered, and stale-session builders cannot persist it.
- Stores metrics in `builder_metrics` table

### POST /api/v1/builders/:id/next-job

Poll for the next available job and advertise builder execution capabilities.
Legacy servers can accept `GET` during a rolling upgrade when the builder also
supports `server_derivation`.

**Response**: `409 Conflict` (preclaim contract conflict)
```json
{
  "reason": "incompatible_evaluator"
}
```

The supported reason values and no-mutation guarantee are defined in the
verified-source contract section above.

**Response**: `200 OK` (job available)
```json
{
  "job_id": "job-uuid",
  "derivation_id": 123,
  "message": "Job assigned"
}
```

**Response**: `200 OK` (no jobs available)
```json
{
  "job_id": null,
  "derivation_id": null,
  "message": "No jobs available"
}
```

**Response**: `200 OK` (at capacity)
```json
{
  "job_id": null,
  "derivation_id": null,
  "message": "Builder at max concurrent job limit"
}
```

**Job Assignment Logic**:
1. Check builder's current active jobs vs `max_concurrent_jobs`
2. If at capacity, return "at limit" response
3. Get builder's environment assignments
4. Query for highest priority queued job matching environments (wildcard if no assignments)
5. Atomically assign job to builder (status → "building", started_at → now)
6. Return job details

**Concurrency**: Uses `FOR UPDATE SKIP LOCKED` to prevent race conditions.

### POST /api/v1/builders/:id/jobs/:job_id/start

Mark job as started (no-op, included for API consistency).

**Request Body**: `{}`

**Response**: `202 Accepted`

**Note**: Job is already marked as "building" when assigned via `next-job`.

### GET /api/v1/builders/:id/jobs/:job_id/derivation-manifest

Fetch the derivation manifest (sorted, deduplicated list of requisite store
paths for the job's drv_path).  Used as the authorization baseline for delta
materialization.

**Authentication**: Required (Ed25519 builder signature)

**Authorization**: Builder must own the job (job.status = "building") with a
matching session ID.

**Response**: `200 OK`
```json
{
  "job_id": "job-uuid",
  "drv_path": "/nix/store/abc123...-hostname.drv",
  "paths": [
    "/nix/store/abc123...-hostname.drv",
    "/nix/store/def456...-source.drv",
    "/nix/store/ghi789...-nixos.drv"
  ]
}
```

**Security**: The manifest is computed server-side from the job's persisted
drv_path.  The builder never supplies the drv_path — this prevents a malicious
or compromised builder from requesting a manifest for a different derivation.

### POST /api/v1/builders/:id/jobs/:job_id/derivation-archive (delta)

Upload a `nix-store --export` archive for a **subset** of the authorized
manifest paths into the builder's local Nix store.

**Authentication**: Required (Ed25519 builder signature)

**Authorization**: Builder must own the job (job.status = "building") with a
matching session ID.

**Request Body**:
```json
{
  "paths": [
    "/nix/store/abc123...-hostname.drv",
    "/nix/store/def456...-source.drv"
  ]
}
```

**Validation**:
- Every path must be in the authorized manifest (computed server-side from the
  job's persisted drv_path).  A path outside the manifest → **403 FORBIDDEN**
  (logged with builder/job IDs; path list is NOT logged to avoid leaking which
  paths a builder was not authorized for).
- Every path must match the pattern `/nix/store/<32-char-hash>-<name>`.  A
  malformed path → **400 BAD REQUEST**.
- Duplicates are silently deduplicated.
- An empty `paths` array → **204 No Content** (all paths are already valid
  locally).

**Response**: `200 OK` — streaming binary body (`application/octet-stream`)
containing the `nix-store --export` output for exactly the validated paths.

**Response**: `204 No Content` — all requested paths already valid locally;
nothing to export.

**Response**: `403 Forbidden` — one or more requested paths are not in the
authorized manifest.  The entire request is rejected; no partial export is ever
served.

**Fallback note from the builder side**: If this endpoint returns 404 (server
too old to support delta protocol), the builder transparently falls back to the
full-archive GET on the same path (see below).  A 403 is never silently
retried as a full archive — that would bypass the authorization check.

### GET /api/v1/builders/:id/jobs/:job_id/derivation-archive (full closure, fallback)

Stream the full `.drv` closure archive into the builder's local Nix store.
This is the **fallback** path used when the server does not support the delta
protocol (always available for backward compatibility).

**Authentication**: Required (Ed25519 builder signature)

**Authorization**: Builder must own the job (job.status = "building") with a
matching session ID.

**Response**: `200 OK` — streaming binary body (`application/octet-stream`)
containing `nix-store --export` of the job's `.drv` recursive closure.  The
response is piped directly into `nix-store --import` on the builder; neither
side buffers the full closure in RAM.

**Use by the builder**:
- Preferred for cold materialization when delta is unavailable.
- The builder should always try the delta POST first; 404/405 causes a
  transparent fallback to this GET.
- A background cache publish (`POST /publish-derivation-closure`) is triggered
  after successful materialization so subsequent builds of the same derivation
  can pull from the binary cache instead.

### POST /api/v1/builders/:id/jobs/:job_id/complete

Report successful build execution and any builder-side publication.

**Request Body** (after a successful push):
```json
{
  "output_path": "/nix/store/0123456789abcdfghijklmnpqrsvwxyz-output",
  "cache_pushed": true,
  "cache_reference": "https://cache.example/read",
  "cache_destination_id": 470
}
```

Niks3 requires the exact destination ID persisted at dispatch and an exact
server-authoritative output. A name or URL cannot replace the ID. Eligible legacy
non-Niks3 reference-only reports remain compatible. An empty body or `{}` remains
a legacy no-push report; updated clients supply the output and omit publication
identity when no push occurred.

Authentication and claim guards run before the read probe. The server performs
`nix path-info --refresh --store <read-store> <exact-output>` before its completion
transaction, with a 60-second metadata deadline. This is exact-root metadata
availability, not a closure copy, NAR-integrity check, or full signature proof.
Niks3 retains protected read credentials, native Basic HTTPS-authority confinement,
configured keys, and `require-sigs` settings. Deployment and CVE materialization
retain their separate artifact-copy checks.

**Response**: `200 OK` (metadata confirmed and publication committed atomically)
```json
{"build_status":"success","publication_status":"confirmed"}
```

**Response**: `200 OK` (no reported cache push)
```json
{"build_status":"success","publication_status":"none"}
```

**Response**: `202 Accepted` (build committed successfully, publication pending)
```json
{
  "build_status": "success",
  "publication_status": "pending",
  "verification_code": "cache_path_missing",
  "retryable": true,
  "retry_after_secs": 2
}
```

The pending transaction rechecks the same builder/session, output, exact selected
destination, eligibility, and mandatory configuration fingerprint as confirmed
completion. It records the existing queue's exact-destination pending publication,
not a completed publication. It retains the recovery GC root and build-log channel.
Invalid ownership remains 403; bad output/dispatch identity, stale configuration,
lease/state mismatch, and cancellation remain hard 403/409 failures as applicable.
These failures never become pending success.

Retryable codes are exactly `cache_path_missing`, `cache_rate_limited`,
`cache_network_error`, and `cache_probe_timeout`. Non-retryable codes are
`cache_authentication_failed`, `cache_read_configuration_invalid`, and
`cache_probe_failed`; their responses omit `retry_after_secs`. Raw Nix stdout,
stderr, URLs, and provider bodies are never response messages. Classification is
a static hint, not authorization or a proven root cause.

The updated builder retries only this POST, at most five requests within ten
minutes, with a 90-second attempt deadline and at least two seconds between
requests. Every attempt signs the same payload/session with a fresh timestamp
and signature. Pending retry requires both an allowlisted transient code and
`retryable: true`. Reporting does not rebuild, re-sign artifacts, or reupload.
An acknowledged pending receipt remains durable build success if a later request
fails. Without any acknowledgement, network failure leaves server state unknown.
The builder does not persist callback recovery across restart, and pending queue
insertion does not guarantee worker startup. After correction, operator-arranged
same-claim callback replay can promote publication idempotently while the job is
`success`; it cannot bypass fingerprint/claim guards or create another build or
scan admission. Recover `cache_pending` publication rather than Retry Build.
See [publication recovery](../caches/niks3-cache.md#publication-failure-and-recovery).

**Side Effects**:
- Status → "success"
- `completed_at` → now
- If post-build scanning is enabled, enqueue or reuse one CVE scan for the exact
  successful derivation in the build-completion transaction. A completion retry
  repairs a missing enqueue idempotently. Reusing active manual or fleet work
  does not replace its trigger provenance. Scan failure does not change build or
  cache status.

The server also runs bounded post-build prerequisite maintenance at startup and
on every CVE worker interval. This maintenance runs even when the local scan
executor is disabled or Vulnix is unavailable. Under the build-derivation lock,
the server binds an existing zero-attempt `awaiting_build` intent to the latest
same-derivation build attempt. Active and successful replacements keep the
intent alive. Only the exact latest failed or cancelled attempt makes the intent
failed. If no authoritative attempt exists, the prerequisite fails as
unavailable. The maintenance does not change manual, fleet, or periodic scans.
It does not consult the current `on_build` policy when it repairs existing
intent; the policy still controls creation of new post-build intent.

Post-build scanning is an event-driven obligation with a bounded recovery
window. Scheduled scanning is an independent freshness mechanism. The policy
field `post_build_recovery_window` defaults to `168h`, counted from the
authoritative successful `build_jobs.completed_at`, not a later scan attempt or
server restart. The server retries unfinished exact post-build work only inside
this window. At expiration it terminalizes persisted unresolved post-build
intent once with `scan_metadata.terminal_reason =
post_build_recovery_window_expired`; it does not manufacture expired scans for
pre-contract historical builds without intent. A missed post-build obligation
is never retried automatically through the post-build path after the window,
but a periodic scan or explicit exact scan can produce later evidence. Existing
execution tokens, lease fencing, and builder session requirements still govern
work in progress; expiration does not revoke an active execution.

### POST /api/v1/builders/:id/jobs/:job_id/fail

Report job failure (triggers retry logic).

**Request Body**:
```json
{
  "status": "failed",
  "error_message": "Build failed: nix-build exited with code 1"
}
```

**Response**: `200 OK` (job re-queued for retry)
**Response**: `202 Accepted` (job permanently failed)

**Retry Logic**:
- If `retry_count < max_retries`:
  - Increment `retry_count`
  - Reduce `priority_weight` by 5%
  - Clear `builder_id` and `started_at`
  - Status → "queued"
  - Return 200
- If `retry_count >= max_retries`:
  - Status → "failed"
  - `completed_at` → now
  - Return 202

### POST /api/v1/builders/:id/jobs/:job_id/logs

Append logs to job.

**Request Body**:
```json
{
  "logs": "Building derivation /nix/store/abc123...\nFetching source...\n"
}
```

**Response**: `202 Accepted`

**Side Effects**:
- Appends `logs` to existing job logs (COALESCE handles NULL initial state)

## Related concepts

* [Builder API: authentication and admin endpoints](builder-api-authentication-and-admin-endpoints.md) - Documents the builder API signature authentication headers and replay window, and the admin endpoints that create, list, update, deactivate, re-key, assign environments to, and read metrics for builders.
* [Builder API: CVE scan endpoints](builder-cve-scan-api.md) - Documents the builder-signed CVE scan endpoints (claim, heartbeat, complete, fail), lease fencing, evidence provenance (server_local_verified and unverified_remote), diagnostics bounds, and scanner process-group rules.
* [Verified-source evaluator contract (source_re_evaluate_verified)](../builders/verified-source-evaluator-contract.md) - Specifies the verified-source flow where the builder re-evaluates a canonical source archive and compares its .drvPath to the server value, including the evaluator fingerprint, next-job 409 reasons, and rolling-upgrade behavior.
* [Builder failure phases and retry strategy](../builders/builder-failure-phases-and-retry.md) - Lists the pre-build failure phases a builder reports (source_fetch through build), which of them retry or fail permanently, and the priority-weighting retry and max-retries rules for build jobs.
* [Builder network flows by execution strategy](../builders/builder-network-flows-by-strategy.md) - Shows the network sequence diagrams for the builder job lifecycle and for ServerDerivation, SourceReEvaluateVerified with ServerBundledArchive, and LocalGitWorktree, including the delta derivation protocol security properties.

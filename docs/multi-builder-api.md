# Multi-Builder API Documentation

> **See also:** [`builder-security-architecture.md`](./builder-security-architecture.md) for the
> complete security architecture, trust boundary diagrams, threat model, and per-strategy
> firewall rules.

## Overview

The Multi-Builder API enables distributed builder deployments with centralized management through the Crystal Forge server. Builders authenticate via Ed25519 signatures and communicate exclusively through REST API endpoints. Builders never access the database directly and never hold repository credentials. When builder-side cache push is enabled, builders may receive narrowly scoped per-job cache push credentials from the server as described in the security boundary notes below.

## Remote Build Execution Strategies

Crystal Forge remote builders use explicit execution strategies. The scheduler must not silently fall back between strategies; a builder only receives jobs for strategies it is configured to support.

### Recommended Default

**Use `source_re_evaluate_verified` + `server_bundled_archive` for all new deployments:**

```toml
# /etc/crystal-forge/server.toml
[server]
remote_build_execution_strategy = "source_re_evaluate_verified"
source_delivery_mode             = "server_bundled_archive"
source_archive_root              = "/var/lib/crystal-forge/source-archives"

# /etc/crystal-forge/builder.toml
[builder]
supported_execution_strategies = ["source_re_evaluate_verified"]
source_worktree_root            = "/var/lib/crystal-forge/flake-worktrees"
allow_import_from_derivation    = true
```

This is the most reliable and fastest startup path because:
- The builder evaluates the flake locally, so there is no dependency on Attic or any binary cache before the build starts.
- The builder checks the evaluated `.drvPath` against the server's expected value before building — giving a build-plan integrity check (`derivation_mismatch` hard failure).
- No Git credentials or direct Git remote access needed on the builder.

Cache publication note: remote builders may perform the post-build cache push themselves. If the selected cache destination requires credentials, the server sends credential-bearing cache push config in the signed next-job response only when `server.trust_forwarded_builder_https` is enabled and the request is verified as HTTPS by the trusted reverse proxy.

For NixOS deployments behind an HTTPS-terminating reverse proxy, enable the
forwarded-HTTPS trust option on the server:

```nix
services.crystal-forge.server.trust_forwarded_builder_https = true;
```

Only enable this when the proxy is under your control and strips/re-sets
forwarding headers before proxying to Crystal Forge. The backend service should
not be directly reachable by builders or untrusted clients over plaintext HTTP.
The proxy must forward one of the HTTPS indicators recognized by the server,
such as `X-Forwarded-Proto: https`, `Forwarded: proto=https`, or
`X-Forwarded-SSL: on`. If this option is left at its secure default of `false`,
credential-bearing builder cache-push jobs are rejected with HTTP `426 Upgrade
Required` before any credentials are sent.

Verified-source evaluator contract version 1 enables Nix
import-from-derivation (IFD) to preserve the authoritative evaluation behavior.
The NixOS module defaults the builder option to true. A builder that disables
IFD must not advertise `source_re_evaluate_verified` contract version 1:

```nix
services.crystal-forge.build.allow_import_from_derivation = true;
```

### `server_derivation` — when to use it

`server_derivation` is simpler and appropriate when you trust the server's evaluation completely and do not need the builder-side re-evaluation check. It is also the only option if the builder cannot run `nix eval` for some reason.

**Materialization:** When the `.drv` is not already in the builder's local Nix store, the builder streams the `.drv` closure archive directly from the CF server into `nix-store --import` — no Attic or binary cache required. In the background, the server pushes the closure to the configured cache so future builds can pull via normal Nix substituters.

**Source delivery for `source_re_evaluate_verified`** is configured server-side via `source_delivery_mode`:

- **`none`**: Not valid for verified-source jobs. The server rejects this mode before claim.

- **`local_git_worktree`**: Reserved for a future evaluator contract. The server rejects this mode before a version-1 job is claimed.

- **`server_bundled_archive`**: Required by contract version 1. During authoritative evaluation, the server creates one uncompressed tracked-tree tar artifact for the exact commit. The server and builder consume the same bytes. Before claim, the server checks the published size and SHA-256 digest. The builder streams those bytes to a unique temporary file, enforces the authorized size, verifies SHA-256, applies bounded safe extraction, and evaluates the resulting Nix store source without Git credentials.

  Materialization schema version 1 accepts full 40-character SHA-1 Git object
  IDs. It rejects 64-character SHA-256 object IDs with a typed unsupported
  object-format error before mirror initialization. A later schema must define
  SHA-256 mirror initialization and compatibility before enabling those IDs.

- **`builder_fetch_public_inputs`**: Reserved for a future evaluator contract. The server rejects this mode before a version-1 job is claimed.

Contract version 1 has no source-delivery fallback. Only
`server_bundled_archive` can claim a `source_re_evaluate_verified` job.

Server publication layout for `server_bundled_archive`:

```
<source_archive_root>/mirrors/<mirror_id>.git
<source_archive_root>/artifacts/<mirror_id>/<commit_hash>.tar
<source_archive_root>/identities/<mirror_id>/<commit_hash>.json
```

### `server_derivation`

The server evaluates the flake, records the authoritative `.drv` path, and sends that derivation identity to the builder.

**Materialization** uses a **delta-aware protocol** by default:

1. Builder checks whether the `.drv` recursive closure is already valid locally via `nix-store --check-validity`.
2. If not, it requests the derivation **manifest** — the server computes `nix-store --query --requisites` from the job's *persisted* drv_path (never a builder-supplied path) and returns the sorted, deduplicated list of store paths.
3. Builder checks local validity of each manifest path (chunked 256/batch with per-path fallback within failed chunks) and requests **only the missing subset** via `POST /derivation-archive { "paths": [...] }`.
4. Server validates every requested path against the authorized manifest and streams `nix-store --export` for exactly that subset. The builder pipes the response into `nix-store --import`.
5. If the server does not support delta endpoints (404/405), the builder transparently falls back to the full closure archive GET.

All streaming is piped stdout with bounded stderr drain — no full closure is buffered in RAM on either side.

Build inputs (nixpkgs, dependencies) are pulled from configured Nix substituters during `nix-store --realise`. Materialization failures are reported as `path_materialization` failures. Builders do not access Postgres directly.

### `source_re_evaluate_verified`

`source_re_evaluate_verified` is the verified source strategy. It keeps the server authoritative while avoiding monolithic derivation-closure transfer as the common path.

Flow:

1. The server fetches the exact commit into its credentialed bare mirror. It
   exports only the tracked Git tree and ingests that tree with the canonical
   `crystal-forge-source-v1-<commit>` name. It evaluates the resulting immutable
   store flake in pure mode with lock mutation disabled and IFD set explicitly:

   ```bash
   nix-eval-jobs --expr '<authoritative expression>' \
       --option pure-eval true \
       --option allow-import-from-derivation true \
       --meta --apply 'derivation: derivation.meta.policies' \
       --workers <n> --max-memory-size <MiB>
   ```

   The resulting `.drvPath` is the server-authorized build-plan fingerprint. The server does not need `nix build --dry-run` for this identity. The authoritative evaluator does not receive `BuildConfig` realization options such as sandbox, offline mode, substitution policy, max jobs, cores, max-silent-time, or build timeout. Those settings control realization and must not alter evaluation semantics.

2. The server sends the full commit, lock digest, canonical store name, source
   NAR hash, artifact format, artifact digest and size, evaluator contract,
   flake target, and expected
   `.drvPath`. The NAR hash and canonical name are the portable source identity.
   The server's physical store path is diagnostic only.

3. The builder obtains the canonical artifact through the job-owned API endpoint.
   The manifest repository URL has embedded user information, passwords, query
   parameters, and fragments removed. The builder does not use the URL to fetch.

4. Before polling, the builder probes and caches its actual Nix version and
   `builtins.currentSystem`. Each signed `NextJobRequest` advertises those values
   with the contract version, pure-evaluation setting, lock-mutation setting,
   IFD setting, and source materialization schema. The server compares the full
   capability with its authoritative `nix-eval-jobs` fingerprint before queue
   lookup or claim. Legacy requests and any mismatch receive HTTP 409 with the
   `incompatible_evaluator` reason and do not mutate a queued job.

5. The builder verifies artifact size, SHA-256, format, lock digest, store name,
   and NAR hash. It rejects incompatible Nix version, purity,
   lock-mutation, IFD, or materialization settings before evaluation. It then
   evaluates the same NAR-qualified store reference as the server before building:

   ```bash
   drv=$(nix eval --raw --no-write-lock-file \
      --option pure-eval true \
      --option allow-import-from-derivation true \
     'path:/nix/store/<source>?narHash=<percent-encoded-SRI>#nixosConfigurations.<host>.config.system.build.toplevel.drvPath')
   ```

6. The builder compares `$drv` to the server-provided expected `.drvPath`. A mismatch fails before any build starts with `derivation_mismatch`.

7. If the strings match, the builder builds the exact verified derivation object:

   ```bash
   nix build "$drv^*"
   ```

   The important property is eval → compare → build, not build → inspect.

This strategy verifies derivation identity/build-plan equality. It does not prove bit-for-bit output reproducibility; output reproducibility is a separate concern.

The evaluator fingerprint covers contract version, linked Nix version,
`builtins.currentSystem`, pure evaluation, lock-file mutation policy, IFD policy,
and source materialization schema. Server-only worker count, evaluator memory
limit, outer process timeout, and cache-status reporting are resource or
diagnostic controls. They can stop an evaluation or add metadata, but they
cannot change a successful `.drvPath`.

A post-claim fingerprint mismatch remains a defense-in-depth check. The server
releases that job to the queue without consuming retry budget or failing the
shared derivation. A pre-upgrade queued job with no usable contract-v1
publication is instead failed with the server-owned `server_failure_code` value
`evaluator_contract_obsolete`; selection continues to the next queue candidate.
Builder logs and failure requests cannot set this field. A later authoritative
re-evaluation can revive the unique row only after source publication and pure
evaluation succeed and the derivation reaches `DryRunComplete`. The same
transaction that queues the row consumes the code by setting it to null. Other
terminal failures retain normal retry and manual-requeue semantics.

Recommended controls:

- Keep source identity immutable: full commit hash, lock digest, artifact format,
  artifact SHA-256 and size, canonical store name, and source NAR hash.
- Retain canonical server artifacts independently of job completion. Dispatch
  validates a published artifact before atomically claiming its exact job.
- Extract builder artifacts only through the bounded contract-v1 tar validator
  and remove each unique temporary directory after evaluation.
- Prefer server-bundled inputs for locked-down or GovCloud-style builders with no internet egress.
- Do not place broad private Git credentials on every builder.
- Record or pin the Nix version/evaluator fingerprint across server and builders.
- Never use `--impure` for this strategy. Impure evaluation can observe host
  `nix.conf`, environment variables, and files and can authorize a host-specific
  derivation.
- P2 hardening: launch authoritative and builder evaluators with an explicit
  environment allowlist so ambient credential variables cannot influence input
  resolution. Contract version 1 does not yet enforce this process boundary.

Expected pre-build failure phases include `source_fetch`,
`source_identity_mismatch`, `evaluator_incompatible`,
`source_input_availability`, `evaluation`, `derivation_mismatch`, and
`path_materialization`.

During rolling upgrades, new builders advertise evaluator contract version 1.
Old builders and old request payloads default to version 0. The server returns
409 before job claim when the configured verified-source strategy requires a
contract the builder cannot validate. It does not silently select another
strategy. A new builder can still poll an old server because old serde readers
ignore the additive capability field.

Every next-job 409 response has a JSON body with a stable `reason` field:

- `unsupported_execution_strategy` means the builder did not advertise the
  server's configured strategy.
- `incompatible_evaluator` means the complete builder and server evaluator
  fingerprints differ.
- `incompatible_source_delivery` means the server delivery mode cannot satisfy
  the selected evaluator contract.
- `source_materialization_cancelled` means canonical source preparation was
  cancelled before claim.

The first three checks occur before queue lookup. Source cancellation occurs
before the atomic claim. None of these responses claims or mutates a queued job.

## Architecture

### Components

- **Server**: Central coordinator managing builder registration, job assignment, and metrics
- **Builder**: Remote build executor that polls for jobs and reports status via API
- **Database**: PostgreSQL schema with builders, job queue, metrics, and environment assignments

### Key Features

- **Authentication**: Ed25519 signature per request (stateless)
- **Authorization**: Admin-only builder management, builder-authenticated work queue
- **Environment Filtering**: Builders assigned to specific environments (or wildcard for all)
- **Concurrent Job Limits**: Configurable max_concurrent_jobs per builder
- **Retry Logic**: Intelligent retry with priority weighting
- **Heartbeat Tracking**: Automatic offline detection and job reassignment

## Database Schema

### `builders` Table

Stores registered builders with resource limits and status.

```sql
CREATE TABLE builders (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT UNIQUE NOT NULL,
    public_key TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('active', 'inactive', 'offline')),
    max_cpu_cores INTEGER,              -- NULL = unlimited
    max_memory_mb INTEGER,              -- NULL = unlimited
    max_concurrent_jobs INTEGER NOT NULL DEFAULT 1,
    last_heartbeat_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
```

### `builder_environment_assignments` Table

Maps builders to environments (1:many relationship).

```sql
CREATE TABLE builder_environment_assignments (
    id SERIAL PRIMARY KEY,
    builder_id UUID NOT NULL REFERENCES builders(id) ON DELETE CASCADE,
    environment_id UUID NOT NULL REFERENCES environments(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE(builder_id, environment_id)
);
```

**Wildcard Behavior**: Builders with zero environment assignments receive jobs from all environments.

### `build_jobs` Table

Job queue with retry logic and priority weighting.

```sql
CREATE TABLE build_jobs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    builder_id UUID REFERENCES builders(id) ON DELETE SET NULL,
    derivation_id INTEGER NOT NULL REFERENCES derivations(id) ON DELETE CASCADE,
    environment_id UUID REFERENCES environments(id) ON DELETE SET NULL,
    status TEXT NOT NULL CHECK (status IN ('queued', 'building', 'success', 'failed')),
    retry_count INTEGER NOT NULL DEFAULT 0,
    max_retries INTEGER NOT NULL DEFAULT 3,
    priority_weight DOUBLE PRECISION NOT NULL DEFAULT 1.0,
    started_at TIMESTAMPTZ,
    completed_at TIMESTAMPTZ,
    logs TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
```

**Job States**:
- `queued`: Available for assignment
- `building`: Assigned to builder, in progress
- `success`: Completed successfully
- `failed`: Permanently failed (exceeded max_retries)

**Retry Logic**:
- Jobs auto-retry on failure if `retry_count < max_retries`
- Priority reduced by 5% per retry (newer commits stay higher priority)
- After max retries, job marked permanently failed

### `builder_metrics` Table

Stores resource usage metrics from builder heartbeats.

```sql
CREATE TABLE builder_metrics (
    id SERIAL PRIMARY KEY,
    builder_id UUID NOT NULL REFERENCES builders(id) ON DELETE CASCADE,
    timestamp TIMESTAMPTZ NOT NULL DEFAULT now(),
    cpu_usage_percent DOUBLE PRECISION NOT NULL,
    memory_usage_mb BIGINT NOT NULL,
    system_cpu_usage_percent DOUBLE PRECISION,
    system_memory_total_mb BIGINT,
    system_memory_used_mb BIGINT
);
```

## API Reference

### Authentication

All builder API endpoints require Ed25519 signature authentication:

**Headers**:
- `X-Builder-ID`: UUID of the builder
- `X-Signature`: Base64-encoded Ed25519 signature of canonical payload
- `X-Timestamp`: RFC 3339 timestamp generated by builder at request time

**Signature Process**:
1. Build canonical payload bytes exactly as: `{METHOD}\n{PATH}\n{TIMESTAMP}\n{RAW_BODY_BYTES}`
2. Sign canonical payload bytes with builder private key (Ed25519)
3. Base64-encode signature and set `X-Signature`
4. Set `X-Timestamp` to the same timestamp used in canonical payload
5. Server verifies signature using builder's stored public key

**Replay Protection**:
- Timestamp must be within +/- 5 minutes of server time.
- Requests outside this freshness window are rejected.

**Authorization**:
- Admin endpoints require authenticated user with admin role
- Builder endpoints require valid builder signature

### Admin Endpoints

All admin endpoints require admin role authorization.

#### POST /api/v1/builders

Create a new builder.

**Request Body**:
```json
{
  "name": "builder-1",
  "public_key": "base64-encoded-ed25519-public-key (optional)",
  "max_cpu_cores": 4,
  "max_memory_mb": 8192,
  "max_concurrent_jobs": 2,
  "environment_ids": ["uuid-1", "uuid-2"]
}
```

If `public_key` is omitted, server generates a new Ed25519 keypair and returns the private key once.

**Response**: `201 Created`
```json
{
  "builder": {
    "id": "builder-uuid",
    "name": "builder-1",
    "public_key": "base64-encoded-ed25519-public-key",
    "status": "inactive",
    "max_cpu_cores": 4,
    "max_memory_mb": 8192,
    "max_concurrent_jobs": 2,
    "last_heartbeat_at": null,
    "created_at": "2026-02-28T10:00:00Z",
    "updated_at": "2026-02-28T10:00:00Z"
  },
  "private_key": "base64-encoded-ed25519-private-key-or-null",
  "assigned_environment_ids": ["uuid-1", "uuid-2"]
}
```

`private_key` is `null` when client supplied `public_key` and is only present once when server generated a keypair.

#### GET /api/v1/builders

List all builders.

**Response**: `200 OK`
```json
[
  {
    "id": "builder-uuid",
    "name": "builder-1",
    "status": "active",
    "max_cpu_cores": 4,
    "max_memory_mb": 8192,
    "max_concurrent_jobs": 2,
    "last_heartbeat_at": "2026-02-28T10:15:00Z",
    "assigned_environment_count": 2
  }
]
```

#### GET /api/v1/builders/:id

Get builder details with environment assignments.

**Response**: `200 OK`
```json
{
  "id": "builder-uuid",
  "name": "builder-1",
  "public_key": "base64-encoded-ed25519-public-key",
  "status": "active",
  "max_cpu_cores": 4,
  "max_memory_mb": 8192,
  "max_concurrent_jobs": 2,
  "last_heartbeat_at": "2026-02-28T10:15:00Z",
  "created_at": "2026-02-28T10:00:00Z",
  "updated_at": "2026-02-28T10:15:00Z",
  "assigned_environment_ids": ["uuid-1", "uuid-2"]
}
```

#### PATCH /api/v1/builders/:id

Update builder configuration.

**Request Body** (all fields optional):
```json
{
  "name": "builder-1-updated",
  "status": "active",
  "max_cpu_cores": 8,
  "max_memory_mb": 16384,
  "max_concurrent_jobs": 4
}
```

**Response**: `200 OK` (updated builder object)

#### DELETE /api/v1/builders/:id

Deactivate a builder (soft delete).

**Response**: `200 OK` (builder with status "inactive")

#### PUT /api/v1/builders/:id/public-key

Update builder's public key.

**Request Body**:
```json
{
  "public_key": "new-base64-encoded-ed25519-public-key"
}
```

**Response**: `200 OK` (updated builder object)

#### PATCH /api/v1/builders/:id/environments

Update builder's environment assignments.

**Request Body**:
```json
{
  "environment_ids": ["uuid-1", "uuid-3"]
}
```

**Response**: `204 No Content`

**Note**: Empty array removes all assignments (wildcard behavior).

#### GET /api/v1/builders/:id/metrics

Get recent metrics for a builder.

**Query Parameters**:
- `limit`: Number of metrics to return (default: 100)

**Response**: `200 OK`
```json
[
  {
    "id": 123,
    "builder_id": "builder-uuid",
    "timestamp": "2026-02-28T10:15:00Z",
    "cpu_usage_percent": 45.2,
    "memory_usage_mb": 2048,
    "system_cpu_usage_percent": 60.5,
    "system_memory_total_mb": 16384,
    "system_memory_used_mb": 8192
  }
]
```

### Builder Endpoints

All builder endpoints require builder signature authentication.

#### POST /api/v1/builders/:id/heartbeat

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

#### POST /api/v1/builders/:id/next-job

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

#### POST /api/v1/builders/:id/jobs/:job_id/start

Mark job as started (no-op, included for API consistency).

**Request Body**: `{}`

**Response**: `202 Accepted`

**Note**: Job is already marked as "building" when assigned via `next-job`.

#### GET /api/v1/builders/:id/jobs/:job_id/derivation-manifest

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

#### POST /api/v1/builders/:id/jobs/:job_id/derivation-archive (delta)

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

#### GET /api/v1/builders/:id/jobs/:job_id/derivation-archive (full closure, fallback)

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

#### POST /api/v1/builders/:id/jobs/:job_id/complete

Mark job as successfully completed.

**Request Body**: `{}`

**Response**: `200 OK`

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

#### POST /api/v1/builders/:id/cve-scans/claim

Claim one schema-1 CVE scan through the authenticated builder session. Builders
that omit the scanner capability remain compatible and receive no scan work.
The server permits one active scan per enabled, registered, active builder and
requires the current process session for every lease mutation. Candidate
selection applies the same wildcard-or-assigned environment rule as build work;
unauthorized work is omitted rather than disclosed. Background claims return no
work while build work is queued or while the builder owns an active build. A
request that names the builder's successful `completed_build_job_id` can claim
only that job's post-build scan.

Remote claims require the exact builder and process session that produced the
successful build. This affinity is the only remote-output locality fact that
the coordinator knows. A completed cache push does not prove that an arbitrary
builder can read the cache or has materialized the output. Manual and fleet
scans therefore remain server-local. Post-build work that the producing session
does not claim remains for the delayed server-local fallback.

The claim contains the exact target `.drv`, output mapping, the builder-reported
scanner identity and version,
bounded scan policy, execution UUID, session UUID, and lease expiration. The
builder must treat all lease fields as opaque server-issued authorization. The
builder passes its startup scanner probe into execution and rejects the lease if
the local name or complete version string differs from the claim.

Every builder-side and server-local `nix`, `nix-store`, and `vulnix` scanner
command runs as the leader of an isolated Unix process group. Timeout, lease
revocation, worker cancellation, future drop, and shutdown signal the complete
group and reap the direct child. Unix init or the configured subreaper reaps
terminated descendants because they are not direct Crystal Forge children.

#### POST /api/v1/builders/:id/cve-scans/heartbeat

Renew the exact active CVE execution. The server returns `410 Gone` when the
lease expired or a new builder session superseded it. Entry and observation
progress above the claim limits also prevents renewal. Upgraded builders may
include up to 16 new or retried single-line phase diagnostics in a heartbeat;
older builders omit the field. The 64 KiB heartbeat body limit and per-message
2,048-character limit bound this live channel. Accepted event types are attempt
start, materialization start/completion, scanner start/completion, and evidence
resolution start/completion. The server renews the fenced lease and appends the
phase events in one transaction. A unique execution-and-event-type identity
makes an uncertain heartbeat retry idempotent. The server acknowledges the
heartbeat only after that transaction commits.

#### POST /api/v1/builders/:id/cve-scans/complete

Submit structured schema-1 package and CVE observations. The request body is
limited to 8 MiB, 50,000 package entries, and 250,000 observations. The server
validates the exact claim identity and submitted output mappings, canonicalizes
ordering and CVE identifiers, recomputes SHA-256, and seals the result through
the same immutable persistence transaction used by local scanning.

All submitted paths must be canonical direct children of `/nix/store`; nested
paths, traversal components, and extra separators are rejected by both builder
and server. When every authorized target output exists in the server's Nix
store, the server requires each submitted package output to be a member of the
server-computed target closure and requires Nix to report the submitted package
`.drv` as that output's exact deriver. The persisted provenance is
`server_local_verified`.

When any target output exists only on the producing builder, the server cannot
reconstruct the closure without materializing it. The server persists
`unverified_remote` in that case. The server still queries the actual deriver
for every submitted package output that exists locally and rejects any mapping
that does not match the submitted package `.drv`. Only unavailable closure
membership and unavailable package-output mappings remain unverified. This
marker is not equivalent to server-local verification; the authenticated
producing builder/session and exact target/output claim remain the explicit
package-list trust boundary. The server does not infer package evidence from
names or grant builders database access.
Affected and whitelisted markers are persisted as independent exact-observation
fields. Builder and server digest the bytes produced by the shared protocol
canonical-result encoder.

Upgraded builders may include a `diagnostics` array in completion and failure
requests. Older builders may omit the field; the server treats an omitted field
as an empty array. The builder applies its shared credential-redaction policy
before it serializes either request. The server applies canonical snapshot
redaction again before persistence as defense in depth. Diagnostics are
operational detail and are excluded from the
canonical schema-1 evidence bytes and SHA-256 digest. A terminal report accepts
at most 256 prepared diagnostic events. Each persisted event contains at most
2,048 Unicode scalar values. The server splits multiline output, normalizes
level, source, and event type, removes control characters, and applies the
canonical snapshot-redaction policy before persistence. Builder-side capture is
also bounded to 64 KiB of stderr and marks truncated output. Diagnostic rows are
append-only and are fenced by the same execution, lease, builder, and current
session checks as the terminal scan transition. A diagnostic failure does not
change build or cache outcomes and diagnostic content never changes CVE evidence.
The detail API orders events by attempt and server-assigned row identity. The
builder-supplied observation time is informational and cannot reorder lifecycle
events.

Invalid evidence returns `422 Unprocessable Entity` and leaves the lease active.
An expired or superseded execution returns `410 Gone`. A same-digest retry is
idempotent; a different digest for the same completed execution returns
`409 Conflict`.

#### POST /api/v1/builders/:id/cve-scans/fail

Report a transient, deterministic, authorization, or cancelled scanner failure.
Transient, authorization, and cancelled failures requeue the scan for the same
producing session or the server-local fallback. Requeue clears typed remote
ownership and sealed claim-input fields before a local worker can claim the row;
the prior remote execution identity remains in audit metadata. Deterministic
failures terminate only the scan. No scan failure changes the successful build
or cache outcome. Failure requests use the same optional, backward-compatible
`diagnostics` array, redaction boundary, event limits, and execution fencing as
completion requests.

#### POST /api/v1/builders/:id/jobs/:job_id/fail

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

#### POST /api/v1/builders/:id/jobs/:job_id/logs

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

## Builder Deployment

### Prerequisites

1. **Builder ID**: Obtain from admin (created via POST /api/v1/builders)
2. **Private Key**: Generate Ed25519 keypair, provide public key to admin
3. **Server URL**: Crystal Forge server API endpoint
4. **Polling Interval**: Recommended 30 seconds

### Keypair Generation

```bash
# Generate Ed25519 keypair
openssl genpkey -algorithm ED25519 -out builder.key
openssl pkey -in builder.key -pubout -out builder.pub

# Extract base64-encoded public key for registration
openssl pkey -in builder.pub -pubin -outform DER | tail -c +13 | base64
```

### Configuration

```toml
[builder]
builder_id = "uuid-from-admin"
private_key_path = "/path/to/builder.key"
server_url = "https://crystal-forge.example.com"
poll_interval_seconds = 30
max_concurrent_jobs = 2
```

### Builder Polling Loop (Pseudocode)

```rust
loop {
    // 1. Send heartbeat with metrics
    send_heartbeat(builder_id, metrics);
    
    // 2. Check concurrent job limit
    if active_jobs.len() >= max_concurrent_jobs {
        sleep(poll_interval);
        continue;
    }
    
    // 3. Poll for next job
    if let Some(job) = poll_next_job(builder_id) {
        // 4. Execute build in background
        spawn_build_job(job, |status, logs| {
            // 5. Stream logs during build
            append_logs(builder_id, job.id, logs);
            
            // 6. Report completion or failure
            match status {
                Success => complete_job(builder_id, job.id),
                Failed(err) => fail_job(builder_id, job.id, err),
            }
        });
    }
    
    sleep(poll_interval);
}
```

## Environment Assignment

### Wildcard Builders

Builders with **zero environment assignments** receive jobs from all environments:

```sql
-- Builder with no assignments (wildcard)
SELECT COUNT(*) FROM builder_environment_assignments WHERE builder_id = 'uuid';
-- Returns 0

-- This builder receives ALL queued jobs, regardless of environment_id
```

### Environment-Specific Builders

Builders assigned to specific environments only receive matching jobs:

```sql
-- Builder assigned to env-1 and env-2
INSERT INTO builder_environment_assignments (builder_id, environment_id)
VALUES ('builder-uuid', 'env-1'), ('builder-uuid', 'env-2');

-- This builder only receives jobs where:
-- environment_id IN ('env-1', 'env-2') OR environment_id IS NULL
```

**Use Cases**:
- **Wildcard**: Development builders that handle all environments
- **Specific**: Production builders isolated to prod environment only

## Retry Strategy

### Priority Weighting

Jobs have a `priority_weight` (default 1.0, higher = higher priority).

**On Retry**:
- Priority reduced by 5%: `new_priority = old_priority * 0.95`
- Ensures newer commits don't wait behind long retry queues
- Strategy: fail X → build Y → retry X → build Z → retry X

**Example**:
```
Job A: priority 1.0  (attempt 1)
Job A: priority 0.95 (attempt 2, after retry)
Job B: priority 1.0  (attempt 1, new commit)
Job A: priority 0.90 (attempt 3, after retry)
```

Queue order: B (1.0), A (0.95), A (0.90)

### Max Retries

Configurable per job (default: 3).

After exceeding max retries:
- Status → "failed"
- Job removed from queue
- Marked permanently failed

## Heartbeat and Offline Detection

### Heartbeat Interval

Recommended: 30 seconds

**Server-Side**:
- `last_heartbeat_at` updated on every heartbeat
- Status → "active" if currently inactive

### Offline Detection

**Future Implementation** (not yet active):
- Query: `SELECT * FROM builders WHERE last_heartbeat_at < now() - interval '90 seconds' AND status = 'active'`
- Mark as "offline"
- Re-queue in-progress jobs assigned to offline builder

## Performance Considerations

### Indexes

Critical indexes for query performance:

```sql
-- Job queue queries
CREATE INDEX idx_build_jobs_queue ON build_jobs(status, priority_weight DESC, created_at ASC)
    WHERE status = 'queued';

-- Active jobs by builder (concurrency tracking)
CREATE INDEX idx_build_jobs_builder_active ON build_jobs(builder_id)
    WHERE status = 'building';

-- Environment filtering
CREATE INDEX idx_build_jobs_environment ON build_jobs(environment_id);
```

### Query Optimization

**Atomic Job Assignment**:
```sql
SELECT * FROM build_jobs
WHERE status = 'queued'
  AND (environment_id = ANY($1) OR environment_id IS NULL)
ORDER BY priority_weight DESC, created_at ASC
LIMIT 1
FOR UPDATE SKIP LOCKED;
```

- `FOR UPDATE`: Locks the row for update
- `SKIP LOCKED`: Skips locked rows (prevents race conditions with multiple builders)

### Metrics Retention

Default: Keep all metrics (no auto-pruning yet)

**Future**: Configurable retention (e.g., 24 hours) with optional aggregation to hourly summaries.

## Security

> For the complete trust model, threat analysis, data-in-transit classification,
> and per-strategy firewall rules, see
> [`builder-security-architecture.md`](./builder-security-architecture.md).

### Authentication

- **Ed25519 signatures**: Per-request, stateless, replay-protected (±5 min timestamp window)
- **Session scoping**: `X-Builder-Session-ID` scopes job operations to the current process lifetime; job ownership is double-checked on every API call
- **Public key storage**: Stored in database, used for signature verification

### Authorization

- **Admin endpoints**: Require authenticated user with admin role
- **Builder endpoints**: Require valid builder signature (active status)
- **Job ownership**: Builders can only operate on jobs they hold in `building` state with a matching session ID

### Network Boundaries

Builders only need outbound access to:
1. The Crystal Forge server (HTTPS, typically 443)
2. Configured Nix binary cache substituters (HTTPS)

Builders never need access to:
- The PostgreSQL database
- Git remotes (when `server_bundled_archive` delivery is configured)
- Other builder hosts
- Managed NixOS hosts (agents and builders are completely separate)

### Builder Credential Boundary

- Database credentials
- Git repository SSH keys or netrc tokens (server holds these)
- OIDC client secrets
- Deployment authorization or keys for managed hosts

Builders may receive narrowly scoped cache push credentials (for example Attic tokens or S3 access/session keys) only for jobs that require builder-side cache publication. Credential-bearing cache config is included in the signed next-job response only when trusted HTTPS forwarding is explicitly configured; otherwise the server rejects the job claim before sending those credentials. Operators must ensure builders cannot bypass the trusted HTTPS reverse proxy and reach the backend service over plaintext.

### Key Management

**Builder Private Keys**:
- Auto-generated by `cf-keygen` on first start at `/var/lib/crystal-forge/builder-api.key`
- File permissions: 600, owned by the `crystal-forge` service user
- Never committed to version control
- Rotate by generating a new keypair and updating via `PUT /api/v1/builders/:id/public-key`

**Public Keys**:
- Stored in database (updatable via API)
- Used only to verify Ed25519 request signatures
- Registering a new public key immediately invalidates requests signed with the old key

## Troubleshooting

### Builder Not Receiving Jobs

1. **Check builder status**: `GET /api/v1/builders/:id`
   - Status should be "active"
   - `last_heartbeat_at` should be recent

2. **Check environment assignments**: 
   - Verify builder has correct environment assignments
   - Or zero assignments for wildcard behavior

3. **Check concurrent job limit**:
   - Query active jobs: `SELECT COUNT(*) FROM build_jobs WHERE builder_id = 'uuid' AND status = 'building'`
   - Compare to `max_concurrent_jobs`

4. **Check job queue**:
   - Verify jobs exist: `SELECT * FROM build_jobs WHERE status = 'queued'`
   - Check environment_id matches builder assignments

### Authentication Failures

1. **Verify signature generation**: Ensure signing canonical payload bytes exactly as `METHOD\nPATH\nTIMESTAMP\nRAW_BODY_BYTES`
2. **Check builder status**: Only "active" builders can authenticate
3. **Verify public key**: Ensure public key in database matches private key
4. **Check headers**: `X-Builder-ID`, `X-Signature`, and `X-Timestamp` must be present
5. **Check timestamp freshness**: Request timestamp must be within +/- 5 minutes of server time

### Jobs Not Retrying

1. **Check retry count**: `SELECT retry_count, max_retries FROM build_jobs WHERE id = 'uuid'`
2. **Verify status**: Should be "queued" if retrying, "failed" if exceeded max

## Migration from Direct Database Access

### Gradual Rollout

1. **Keep existing builder running**: Direct DB access continues working
2. **Deploy API infrastructure**: Merge backend changes
3. **Register builders in UI**: Create builder records
4. **Update builder binary**: Switch to API client (Phase 6)
5. **Migrate jobs**: Optional - move existing jobs to new build_jobs table

### Backward Compatibility

Current implementation:
- New tables added (builders, build_jobs, etc.)
- Existing tables unchanged (build_reservations extended with FK)
- Existing builder can continue using direct DB access during transition

## Future Enhancements

- **Load-based assignment**: Select least busy builder (track CPU/memory usage)
- **Heartbeat timeout automation**: Auto-mark offline, requeue jobs
- **Metrics aggregation**: Hourly summaries for long-term storage
- **Builder auto-scaling**: Spawn/terminate builders based on queue depth
- **Build cache management**: Shared cache between builders
- **Builder health checks**: Beyond heartbeat (e.g., test builds)

## References

- Task: TASK-140
- Migration: `migrations/0083_create_builders_infrastructure.sql`
- Models: `src/models/builders.rs`
- Queries: `src/queries/builders.rs`
- Handlers: `src/handlers/api/builders.rs`
- Authentication: `src/handlers/builder_request.rs`

# Multi-Builder API Documentation

> **See also:** [`builder-security-architecture.md`](./builder-security-architecture.md) for the
> complete security architecture, trust boundary diagrams, threat model, and per-strategy
> firewall rules.

## Overview

The Multi-Builder API enables distributed builder deployments with centralized management through the Crystal Forge server. Builders authenticate via Ed25519 signatures and communicate exclusively through REST API endpoints. Builders never access the database directly and never hold repository credentials.

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
source_mirror_root              = "/var/lib/crystal-forge/flake-mirrors"
source_worktree_root            = "/var/lib/crystal-forge/flake-worktrees"
cleanup_source_worktrees        = true
```

This is the most reliable and fastest startup path because:
- The builder evaluates the flake locally, so there is no dependency on Attic or any binary cache before the build starts.
- The builder checks the evaluated `.drvPath` against the server's expected value before building — giving a build-plan integrity check (`derivation_mismatch` hard failure).
- No Git credentials or direct Git remote access needed on the builder.

### `server_derivation` — when to use it

`server_derivation` is simpler and appropriate when you trust the server's evaluation completely and do not need the builder-side re-evaluation check. It is also the only option if the builder cannot run `nix eval` for some reason.

**Materialization:** When the `.drv` is not already in the builder's local Nix store, the builder streams the `.drv` closure archive directly from the CF server into `nix-store --import` — no Attic or binary cache required. In the background, the server pushes the closure to the configured cache so future builds can pull via normal Nix substituters.

**Source delivery modes for `source_re_evaluate_verified`** are configured server-side via `source_delivery_mode`:

- **`local_git_worktree`** (default): Builder manages its own bare mirror. On first use it clones with `git clone --bare` from the repository URL; if the authorized commit is absent it fetches. The builder needs read access to the repository URL and credentials for private repos. Colocated server/builder deployments may share the same mirror root.

- **`server_bundled_archive`**: Server packages the top-level flake repository as a `tar.gz` (from its own server-side bare mirror) and serves it via an authenticated API endpoint. The builder downloads, verifies SHA-256 incrementally while streaming to disk, extracts to a **job-scoped** directory, and evaluates without contacting the Git remote. Each job gets an isolated mirror directory so concurrent builds for the same repo do not interfere. Use this for air-gapped or GovCloud builders. Note: only the top-level repo is bundled; locked flake inputs not in the builder's Nix store or substituters may still require network during `nix eval`.

Job-scoped mirror layout for `server_bundled_archive`:

```
<source_mirror_root>/server-bundled/<job_id>/<mirror_id>.git   ← deleted after build
<source_worktree_root>/<mirror_id>/<commit_hash>/<job_id>/     ← deleted after build
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

1. The server evaluates the target with the equivalent of:

   ```bash
   nix eval --raw .#nixosConfigurations.<host>.config.system.build.toplevel.drvPath
   ```

   The resulting `.drvPath` is the server-authorized build-plan fingerprint. The server does not need `nix build --dry-run` for this identity.

2. The server sends a job manifest containing immutable source identity, flake target, source/input delivery mode, evaluator fingerprint, and the expected server `.drvPath`.

3. The builder obtains the immutable source without broad/reusable Git credentials. The preferred operational model is a local Git mirror plus detached worktree:

    ```text
    /var/lib/crystal-forge/flake-mirrors/<mirror-id>.git
    /var/lib/crystal-forge/flake-worktrees/<mirror-id>/<commit-sha>/<job-id>
    ```

    The server serves enough source metadata or snapshot data for the builder to keep its local mirror current. The builder creates a detached per-job worktree at the exact authorized commit. If server and builder are colocated, both may point at the same mirror root to avoid duplicate clone storage; job worktrees remain builder-managed and are cleaned independently.

   Locked-down deployments can still choose a server-bundled source/input archive (for example, a `nix flake archive`/NAR-style artifact). For public inputs, a deployment may allow the builder to fetch public flake inputs itself.

4. The builder verifies the local worktree HEAD equals the manifest commit, then evaluates before building:

   ```bash
   drv=$(nix eval --raw <source>#nixosConfigurations.<host>.config.system.build.toplevel.drvPath)
   ```

5. The builder compares `$drv` to the server-provided expected `.drvPath`. A mismatch fails before any build starts with `derivation_mismatch`.

6. If the strings match, the builder builds the exact verified derivation object:

   ```bash
   nix build "$drv^*"
   ```

   The important property is eval → compare → build, not build → inspect.

This strategy verifies derivation identity/build-plan equality. It does not prove bit-for-bit output reproducibility; output reproducibility is a separate concern.

Recommended controls:

- Keep source identity immutable: commit hash, lock/source metadata, and archive hash where available.
- Prefer detached worktrees from a local mirror over mutable branch checkouts.
- Verify the worktree `HEAD` equals the manifest commit before evaluation.
- Clean up job/commit worktrees after build completion and cache-push/reporting lifecycle is complete.
- Prefer server-bundled inputs for locked-down or GovCloud-style builders with no internet egress.
- Do not place broad private Git credentials on every builder.
- Record or pin the Nix version/evaluator fingerprint across server and builders.
- Disable lockfile mutation and avoid impure evaluation for this strategy.

Expected pre-build failure phases include `source_fetch`, `source_input_availability`, `evaluation`, `derivation_mismatch`, and `path_materialization`.

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
- Marks builder as "active" if previously inactive
- Stores metrics in `builder_metrics` table

#### GET /api/v1/builders/:id/next-job

Poll for next available job.

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

### What Builders Never Hold

- Database credentials
- Git repository SSH keys or netrc tokens (server holds these)
- OIDC client secrets
- Nix cache push credentials (server pushes; builder only pulls)
- Deployment authorization or keys for managed hosts

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

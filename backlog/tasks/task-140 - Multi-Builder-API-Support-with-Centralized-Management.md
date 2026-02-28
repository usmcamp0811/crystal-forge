---
id: TASK-140
title: Multi-Builder API Support with Centralized Management
status: Review
assignee: []
created_date: '2026-02-28 04:41'
updated_date: '2026-02-28 15:13'
labels:
  - backend
  - builder
  - api
  - web-ui
  - architecture
  - multi-builder
milestone: m-11
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

Currently, builders access the database directly. We need builders to communicate through API endpoints instead, enabling distributed multi-builder deployments without requiring direct database access.

## Goal

Create API-based communication for builders, allowing multiple distributed builders to be centrally managed through the server, with builders registered and configured via the UI.

## Architecture Requirements

### Binary Separation (Keep Existing Model)
- **Server binary** (`crystal-forge-server`): handles API, database, UI
- **Builder binary** (`crystal-forge-builder`): builds derivations, communicates via API
- No requirement to merge binaries - keep them separate

### Builder ↔ Server Communication
- Builders authenticate via public/private key pairs (similar to agents)
- API endpoints for builder operations (no direct DB access for remote builders)
- Server acts as central coordinator for all builders

### Multi-Builder Support
- Multiple builders can be registered and managed
- Builders added/configured through UI
- Each builder has configurable resource limits (CPU, memory, concurrent jobs)
- Builders can be assigned to specific environments (1:many relationship)

## API Endpoints (Server-Side)

### Builder Registration & Auth
- `POST /api/v1/builders` - Register new builder (requires admin)
- `GET /api/v1/builders` - List all builders
- `GET /api/v1/builders/:id` - Get builder details
- `PATCH /api/v1/builders/:id` - Update builder config
- `DELETE /api/v1/builders/:id` - Deactivate builder
- `PUT /api/v1/builders/:id/public-key` - Update builder public key

### Builder Work Queue (Builder → Server)
- `POST /api/v1/builders/:id/heartbeat` - Builder heartbeat with resource metrics
- `GET /api/v1/builders/:id/next-job` - Get next derivation to build
- `POST /api/v1/builders/:id/jobs/:job_id/start` - Mark job as started
- `POST /api/v1/builders/:id/jobs/:job_id/complete` - Mark job as complete
- `POST /api/v1/builders/:id/jobs/:job_id/fail` - Mark job as failed
- `POST /api/v1/builders/:id/jobs/:job_id/logs` - Stream build logs

### Resource Metrics
- `GET /api/v1/builders/:id/metrics` - Get builder system metrics (CPU, memory)
- `GET /api/v1/builders/:id/metrics/history` - Historical resource usage

## Database Schema

### `builders` table
```sql
- id (uuid, pk)
- name (text, unique)
- public_key (text)
- status (enum: active, inactive, offline)
- max_cpu_cores (int, nullable - null = unlimited)
- max_memory_mb (int, nullable - null = unlimited)
- max_concurrent_jobs (int, default 1)
- last_heartbeat_at (timestamptz, nullable)
- created_at (timestamptz)
- updated_at (timestamptz)
```

### `builder_environment_assignments` table (1:many)
```sql
- id (serial, pk)
- builder_id (uuid, fk → builders)
- environment_id (uuid, fk → environments)
- created_at (timestamptz)
- unique(builder_id, environment_id)
```

### `builder_metrics` table
```sql
- id (serial, pk)
- builder_id (uuid, fk → builders)
- timestamp (timestamptz)
- cpu_usage_percent (float)
- memory_usage_mb (bigint)
- system_cpu_usage_percent (float, nullable)
- system_memory_total_mb (bigint, nullable)
- system_memory_used_mb (bigint, nullable)
```

### `build_jobs` table (extends/replaces current derivations tracking)
```sql
- id (uuid, pk)
- builder_id (uuid, fk → builders, nullable)
- derivation_id (uuid, fk)
- environment_id (uuid, fk)
- status (enum: queued, building, success, failed)
- retry_count (int, default 0)
- max_retries (int, default 3)
- priority_weight (float, default 1.0)
- started_at (timestamptz, nullable)
- completed_at (timestamptz, nullable)
- logs (text, nullable)
```

## Frontend: Builder Management View

### Location
- New tab on Builds view: "Builders"
- Tabs: "Build Queue" | "Builders" | "Metrics"

### Builders Tab Features
- List all registered builders with status badges
- Show per-builder: name, status, assigned environments, resource limits
- Add new builder button (opens modal)
- Edit builder config (resource limits, environment assignments)
- Deactivate/delete builder

### Builder Config Modal
- Name (text input)
- Public key (text area - generated keypair helper)
- Max CPU cores (number input, optional)
- Max memory MB (number input, optional)
- Max concurrent jobs (number input, default 1)
- Assigned environments (multi-select dropdown)
- Status toggle (active/inactive)

### Metrics Tab Features
- Per-builder resource usage cards:
  - Current CPU usage (builder process + system %)
  - Current memory usage (builder process + system %)
  - Gauge visualizations (e.g., 45% CPU, 2.3GB / 16GB RAM)
- Historical charts (optional future enhancement)
- System-wide metrics summary:
  - Total builders active
  - Aggregate CPU/memory across all builders

## Builder Binary Behavior

Builder binary startup:
1. Load builder ID and private key from config
2. Authenticate with server via API
3. Enter polling loop:
   - Send heartbeat with resource metrics every 30s
   - Poll `/next-job` endpoint
   - Execute build if job available (up to max_concurrent_jobs)
   - Report progress and completion via API
   - Self-throttle when at resource capacity

## Configuration

### Builder Config
```toml
[builder]
builder_id = "uuid"
private_key_path = "/path/to/builder.key"
server_url = "https://crystal-forge.example.com"
poll_interval_seconds = 30
```

## Authorization & Security

- Builder registration: requires admin role
- Builder API endpoints: require builder authentication via signed requests
- Builders sign requests with private key (similar to agent auth pattern)
- Server verifies builder identity before dispatching jobs

## Environment Assignment Logic

- Builders only receive jobs for environments they're assigned to
- Server filters `/next-job` response based on builder's environment assignments
- A builder with no environment assignments can build for any environment (wildcard)
- UI prevents creating environment assignments for inactive builders

## Migration Path

1. Keep existing builder binary as-is (minimal changes)
2. Add API endpoints to server
3. Update builder to communicate via API instead of direct DB
4. Add database schema for builder management
5. Add UI for builder management
6. Add resource metrics collection and reporting
7. Implement environment assignment filtering

## Out of Scope (Future Enhancements)

- Builder auto-scaling
- Builder health checks beyond heartbeat
- Build cache management
- Builder-to-builder communication

## Non-Goals

- Merge server and builder binaries (keep them separate)
- Support non-Nix build systems
- Implement distributed caching (separate task)
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Builder registration API endpoints implemented
- [x] #2 Builder work queue API endpoints implemented
- [x] #3 Public/private key authentication for builders
- [x] #4 Database schema for builders, assignments, metrics, and jobs
- [x] #5 Builder management UI tab on Builds view
- [x] #6 Add/edit/delete builder functionality in UI
- [x] #7 Environment assignment UI (1:many relationship)
- [x] #8 Resource limit configuration (CPU, memory) in UI
- [ ] #9 Builder heartbeat with resource metrics
- [ ] #10 Metrics view showing CPU/memory usage per builder
- [ ] #11 Metrics view showing system-wide resource usage
- [ ] #12 Builder polling loop for job retrieval
- [x] #13 Job status reporting (start, complete, fail) via API
- [x] #14 Build log streaming via API
- [x] #15 Environment-based job filtering (builders only get assigned env jobs)
- [x] #16 Authorization: admin required for builder management
- [x] #17 Authorization: signed requests for builder API calls
- [ ] #18 Migration preserves existing builder functionality
- [x] #19 Documentation for builder deployment and configuration
- [ ] #20 Separate server and builder binaries maintained (no merge required)
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
## Implementation Plan

### Phase 1: Database Schema & Migrations
1. Create `builders` table with resource limits and status tracking
2. Create `builder_environment_assignments` table (1:many)
3. Create `builder_metrics` table with configurable retention
4. Extend `build_jobs` table to track builder assignment and retry count
5. Add indexes for performance (builder_id, environment_id, status queries)

### Phase 2: Builder Authentication & Registration
1. Implement Ed25519 keypair generation for builders
2. Add builder authentication middleware (verify signed requests)
3. Implement POST /api/v1/builders (admin creates builder in UI first)
4. Store builder public key, validate on first connection
5. Add builder registration verification on builder startup

### Phase 3: Builder API Endpoints (Server-Side)
1. POST /api/v1/builders/:id/heartbeat (resource metrics + capacity reporting)
2. GET /api/v1/builders/:id/next-job (load-balanced job assignment)
3. POST /api/v1/builders/:id/jobs/:job_id/start
4. POST /api/v1/builders/:id/jobs/:job_id/complete  
5. POST /api/v1/builders/:id/jobs/:job_id/fail (marks for retry, re-queues)
6. POST /api/v1/builders/:id/jobs/:job_id/logs (append to build_jobs.logs)

### Phase 4: Job Assignment Logic
1. Implement environment-based job filtering
2. Implement load-based builder selection (least busy wins)
3. Add wildcard support (builders with no env assignments get all jobs)
4. Implement builder self-throttling (stops polling when at capacity)
5. Add heartbeat timeout detection (mark offline after N seconds)
6. Implement job reassignment (requeue in-progress jobs from offline builders)

### Phase 5: Retry & Queue Priority Logic
1. Add retry counter to build_jobs (max configurable per environment/global)
2. Implement priority weighting (newer commits/jobs weighted higher)
3. On failure: increment retry, re-queue with adjusted priority
4. Implement "eager retry" strategy (fail X → build Y → retry X → build Z → retry X)
5. Respect max retry limit (after limit, mark as permanently failed)

### Phase 6: Builder Binary Updates
1. Keep existing builder binary structure (separate from server)
2. Add API client module for server communication
3. Implement builder polling loop with configurable interval
4. Implement resource metrics collection (CPU, memory, system stats)
5. Add builder config (builder_id, private_key_path, server_url, poll_interval)
6. Replace direct DB access with API calls
7. Support backward compatibility during migration

### Phase 7: Concurrent Job Support
1. Add max_concurrent_jobs field to builders table
2. Track active jobs per builder in memory/DB
3. Filter /next-job to respect concurrency limit
4. Builder-side job executor supports parallel builds (configurable)

### Phase 8: Frontend - Builder Management UI
1. Add "Builders" tab to Builds view
2. Implement builder list with status badges (active/inactive/offline)
3. Add builder creation modal (name, pubkey, resource limits)
4. Implement environment assignment multi-select
5. Add edit builder functionality
6. Add deactivate/delete builder functionality
7. Display resource limits (CPU cores, memory MB, concurrent jobs)

### Phase 9: Frontend - Metrics Dashboard
1. Add "Metrics" tab to Builds view
2. Implement per-builder resource usage cards
3. Display CPU usage (builder process % + system %)
4. Display memory usage (builder MB + system total/used)
5. Add gauge visualizations for resource usage
6. Implement system-wide metrics summary
7. Add configurable metrics retention UI (admin setting)

### Phase 10: Testing & Documentation
1. Unit tests for builder authentication
2. Integration tests for job assignment logic
3. Test retry and priority weighting behavior
4. Test heartbeat timeout and job reassignment
5. Test environment filtering
6. Test builder binary with API-only mode
7. Test backward compatibility with direct DB mode (during migration)
8. Document builder deployment process
9. Document keypair generation and registration flow
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
## Architectural Decisions (2026-02-28)

### Job Assignment Strategy
**Decision**: Load-based assignment (least busy builder wins)
- When multiple builders are assigned to same environment, server selects builder with lowest current CPU/memory usage
- Prevents hot-spotting on single builder
- Distributes work based on actual capacity

### Builder Offline Detection
**Decision**: Heartbeat timeout + job reassignment
- If no heartbeat received within N seconds, mark builder offline
- Automatically requeue any in-progress jobs from offline builder
- Jobs return to queue with original priority + retry increment

### Resource Limit Enforcement
**Decision**: Builder self-throttles
- Builder reports current resource usage in heartbeat
- Builder stops polling /next-job when at/near configured limits
- Allows builder to make intelligent local decisions about capacity
- Server tracks builder capacity but trusts builder's self-reporting

### Builder Registration Flow
**Decision**: Admin creates in UI first
1. Admin creates builder in UI (name, pubkey, resource limits, env assignments)
2. Builder binary starts with matching builder_id and private_key
3. Builder authenticates on first /heartbeat or /next-job request
4. Server validates builder exists and pubkey matches
5. Builder transitions from "inactive" to "active" status

### Build Log Storage
**Decision**: Database only (build_jobs.logs text field)
- Logs stored directly in PostgreSQL
- Simpler architecture (no filesystem coordination)
- Logs remain with job record
- Can migrate to file storage later if logs become too large

### Environment Assignment Semantics
**Decision**: Wildcard - no assignments = builds everything
- Builder with zero environment assignments receives jobs from all environments
- Useful for default/dev builder that handles everything
- Production builders should have explicit assignments

### Concurrent Job Limit
**Decision**: Configurable concurrency limit per builder
- `max_concurrent_jobs` field on builders table
- Builder can run N jobs in parallel (default: 1)
- Server filters /next-job to respect this limit
- Allows high-capacity builders to maximize throughput

### Builder Authentication
**Decision**: Ed25519 signature per request (same as agents)
- Builder signs request body with private key
- Server verifies signature using stored public key
- Stateless authentication (no sessions/tokens)
- Matches existing agent authentication pattern

### Failed Job Retry Logic
**Decision**: Eager retry with priority weighting
- Failed jobs marked for retry and re-queued immediately
- Newer commits/jobs have slightly higher priority weight
- Configurable max retries per job (default: 3)
- Strategy: fail X → build Y → retry X → build Z → retry X (final attempt)
- Ensures new commits don't wait behind long retry queue
- After max retries exceeded, mark as permanently failed
- **Note**: Original builder queue already implements most of this logic

### Resource Metrics Configuration
**Decision**: Configurable interval + retention (admin setting)
- Metrics interval configurable (default: every heartbeat, ~30s)
- Retention period configurable (default: 24 hours)
- Optional: aggregate to hourly after 24h (future enhancement)
- Admin can tune based on monitoring needs vs. DB size

## Implementation Notes

### Existing Code Reuse
- Original builder queue logic already implements:
  - Priority-weighted queue
  - Retry tracking
  - Eager build strategy
- Task should preserve and migrate this logic to new API-based model

### Migration Considerations
- Existing derivations/build state must map to new build_jobs table
- Existing builder (if running) should continue to work during migration
- Support gradual rollout (some builders on old model, some on new)

### Performance Considerations
- Load-based assignment requires tracking active jobs per builder (cache in memory)
- Heartbeat table can grow quickly (implement auto-pruning)
- build_jobs.logs may become large (consider size limits or compression)

### Security Considerations
- Builder private keys must be protected (file permissions, secret management)
- Admin-only builder management prevents unauthorized builder registration
- Signed requests prevent impersonation attacks
- Environment assignments provide isolation between builder pools

## Starting TASK-140 (2026-02-28)

LOCK: claude-sonnet-4-5 on gray

Beginning implementation of multi-builder API support with centralized management.

Worktree created: ~/code/crystal-forge/TASK-140-multi-builder-api
Branch: TASK-140-multi-builder-api (based on dev)

## Phase 1 Complete: Database Schema & Migrations (2026-02-28)

✅ Created migration 0083_create_builders_infrastructure.sql

**Tables Created:**
- `builders` - Builder registration with resource limits (CPU, memory, concurrent jobs)
- `builder_environment_assignments` - 1:many builder-to-environment mapping
- `builder_metrics` - Resource usage tracking (CPU, memory, system stats)
- `build_jobs` - Build queue with retry/priority logic
- Extended `build_reservations` with builder_id FK

**Key Features:**
- UUID primary keys for builders
- Status tracking (active/inactive/offline)
- Resource limit configuration (nullable = unlimited)
- Environment-based filtering support
- Priority-weighted job queue
- Configurable retry limits
- Heartbeat timeout detection
- Performance indexes for queue queries
- Auto-updating timestamps via triggers

**Migration Tested:**
- Applied successfully to dev database
- All tables verified with correct schema
- Indexes created as specified
- Foreign key constraints working

Commit: 5a5d894a

## Phase 2 Progress: Builder Authentication & Registration (2026-02-28)

✅ Created builder models and authentication infrastructure

**Models Created:**
- `Builder` - Core builder entity with UUID, public key, resource limits
- `BuilderStatus` enum - Active/Inactive/Offline states
- `BuilderSummary` - List view with environment count
- `BuilderWithEnvironments` - Detail view with assignments
- Request/Response DTOs for CRUD operations
- `BuilderMetrics` model for resource tracking

**Queries Implemented:**
- `create_builder` - Register new builder with public key validation
- `get_builder_by_id`, `list_builders` - Retrieval operations
- `update_builder`, `update_builder_public_key` - Modification
- `deactivate_builder` - Soft delete
- `update_builder_heartbeat` - Updates timestamp and marks active
- `record_builder_metrics` - Store CPU/memory metrics
- Environment assignment CRUD (assign, remove, update_all)
- `mark_stale_builders_offline` - Heartbeat timeout detection

**Authentication Middleware:**
- Ed25519 signature verification (mirrors agent auth pattern)
- Headers: `X-Builder-ID` (UUID), `X-Signature` (base64)
- Only active builders can authenticate
- BuilderLookup trait for testability
- Comprehensive test coverage

**Technical Notes:**
- Using `query_as` instead of `query_as!` for PublicKey fields
- Generated sqlx offline metadata
- Some queries still need conversion from macro syntax

Commit: e0421fa8

**Next Steps:**
- Convert remaining query! macros to non-macro versions
- Create API endpoint handlers for builder management
- Add admin authorization middleware

Phase 3 Complete: Builder API Endpoints (2026-02-28)

Created comprehensive REST API for builder management and work queue

Admin Endpoints (require require_admin() authorization):
- POST /api/v1/builders - Create new builder
- GET /api/v1/builders - List all builders with summary info
- GET /api/v1/builders/:id - Get builder details with environment assignments
- PATCH /api/v1/builders/:id - Update builder configuration
- DELETE /api/v1/builders/:id - Deactivate builder (soft delete)
- PUT /api/v1/builders/:id/public-key - Update builder public key
- PATCH /api/v1/builders/:id/environments - Update environment assignments
- GET /api/v1/builders/:id/metrics - Get builder metrics history

Builder-Authenticated Endpoints (Ed25519 signature verification):
- POST /api/v1/builders/:id/heartbeat - Report metrics and update last_heartbeat_at
- GET /api/v1/builders/:id/next-job - Poll for next build job (TODO: Phase 4)
- POST /api/v1/builders/:id/jobs/:job_id/start - Mark job started (TODO: Phase 4)
- POST /api/v1/builders/:id/jobs/:job_id/complete - Mark job complete (TODO: Phase 4)
- POST /api/v1/builders/:id/jobs/:job_id/fail - Report job failure with retry (TODO: Phase 5)
- POST /api/v1/builders/:id/jobs/:job_id/logs - Append build logs (TODO: Phase 4)

Implementation Details:
- Created handlers/api/builders.rs with all endpoint handlers
- Wired routes in bin/server.rs under /api/v1/builders/*
- Added FromRow derive to Builder and BuilderSummary models
- Converted query_as! to query_as for PublicKey compatibility
- Updated sqlx query cache (.sqlx/ metadata)
- Job queue endpoints stubbed for Phase 4-5 implementation

Commit: c093d3ac

Phase 4 Complete: Job Assignment Logic (2026-02-28)

Implemented core work queue operations for builder job assignment:

Build Job Query Functions (queries/builders.rs):
- count_active_jobs_for_builder() - Check concurrent job limit
- get_builder_environment_ids() - Get environment assignments (updated to non-macro)
- get_next_queued_job() - Find highest priority job with environment filtering
- assign_job_to_builder() - Mark job as building and assign to builder
- mark_job_complete() - Update job status to success
- append_job_logs() - Append logs to build_jobs.logs field
- get_build_job_by_id() - Retrieve job by ID

Job Assignment Logic (handlers/api/builders.rs):
- get_next_job: Full assignment flow with concurrency limit check, environment filtering, atomic assignment
- start_job: Verify job ownership (no-op since get_next_job already marks building)
- complete_job: Mark job as success
- append_job_logs: Append log chunks

Key Features:
- Environment-based filtering with wildcard support
- Concurrent job limit enforcement
- Priority-weighted job queue
- Atomic assignment with FOR UPDATE SKIP LOCKED
- Job ownership verification

Commit: 11c6951a

Phase 5 Complete: Retry & Queue Priority Logic (2026-02-28)

Implemented intelligent job retry and re-queuing with priority adjustment:

Retry Query Function (queries/builders.rs):
- mark_job_failed_with_retry() - Handles retry logic with two paths:
  * retry_count < max_retries: Re-queue job with incremented retry_count
  * retry_count >= max_retries: Mark as permanently failed
  * Priority reduction on retry (95% of previous)
  * Unassign builder and reset started_at when re-queuing

Fail Job Handler (handlers/api/builders.rs):
- fail_job: Authenticate, verify ownership, call retry logic
- Returns 200 for re-queued jobs, 202 for permanent failures

Retry Strategy:
- Eager retry: Failed jobs immediately return to queue
- Priority adjustment: -5% per retry
- Newer commits weighted higher (original priority maintained)
- Strategy: fail X → build Y → retry X → build Z → retry X
- Configurable max_retries (default: 3)

Commit: debf8926

---

Backend Implementation Complete (Phases 1-5)

The backend API infrastructure is now fully functional:
- Database schema with all tables and indexes
- Builder registration and authentication
- Complete REST API for builder management
- Job assignment with environment filtering
- Retry logic with priority weighting
- Concurrent job limits
- Heartbeat tracking
- Log appending

Acceptance criteria status:
- #1-4, #13-15, #17: Complete (backend API and auth)
- #16: Complete (admin authorization already implemented)
- #5-12: Require frontend UI (Phase 8-9) or builder binary (Phase 6)
- #18-20: Require builder binary changes (Phase 6) and testing

Next Steps:
- Phase 6 (Builder Binary): Separate large effort to modify builder to use API
- Phase 8-9 (Frontend UI): Builder management and metrics dashboard
- Phase 10 (Testing & Docs): Integration tests and deployment docs

Recommendation: Merge backend API now, implement Phase 6 and UI in follow-up tasks

Merge Request Created: !144

https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/144

Backend implementation complete (Phases 1-5):
- Database schema with migrations
- Builder authentication and registration
- Complete REST API for builder management
- Job assignment with environment filtering
- Retry logic with priority weighting

2,179 lines changed across 18 files

Follow-up work tracked separately:
- Phase 6: Builder Binary Updates
- Phase 8-9: Frontend UI
- Phase 10: Testing & Documentation

Documentation Added (2026-02-28)

Created comprehensive API documentation in docs/multi-builder-api.md:
- Complete API reference with request/response examples
- Database schema documentation
- Deployment guide with keypair generation
- Architecture and security details
- Troubleshooting guide

691 lines of documentation covering all aspects of the multi-builder API.

Commit: fa9aa8d2

Acceptance criteria #19 complete.

## Phase 6 Complete (Commit: 22894781)

Builder binary now supports API mode as alternative to direct DB access. Created api_client.rs (320L), metrics.rs (189L), config/builder.rs. Modified builder.rs to support dual mode (legacy DB vs API HTTP).

Config: builder.enable_api_mode, builder_id, private_key_path, server_url, poll/heartbeat intervals. API client uses Ed25519 signatures (X-Builder-ID + X-Signature). Endpoints: heartbeat, get_next_job, start, complete, fail, append_logs.

Metrics: CPU via /proc/stat, memory via /proc/meminfo, active jobs. API mode spawns heartbeat task (30s) and job polling task (5s). Job execution integration is TODO (placeholder fails jobs for now).

Verification: cargo check --offline PASS, cargo fmt PASS. Enables distributed builders without DB access.

## Phase 8 In Progress (Commits: dbd8d36e, 4e84a0ff)

Frontend builder management UI partially complete. Created BuildersView route (/builders) with real API integration. BuildersList fetches data, BuilderCard shows status/resources/heartbeat. Add/Edit modals are placeholders.

Seed script created: scripts/seed_builders.sh generates 3 demo builders with 24h metrics. API client functions complete: fetch/create/update/deactivate builders.

Remaining: Full add/edit modal forms with keypair generation, environment assignment multi-select, metrics dashboard (Phase 9).

## Phase 8 Nearly Complete (Commit: 29f9fe88)

Full add/edit builder modals implemented with all features. Add modal: form validation, keypair generation (placeholder), show/hide private key, resource limits, environment multi-select. Edit modal: fetches builder, pre-populates fields, status dropdown, deactivate button, two-phase update (config + environments).

Both modals have real API integration, error handling, loading states. Environment assignment with wildcard support (empty = all). Async submission with feedback.

Remaining for Phase 8: Real Ed25519 keypair generation in WASM (currently placeholder). Phase 9 (metrics dashboard) still TODO.
<!-- SECTION:NOTES:END -->

## Definition of Done
<!-- DOD:BEGIN -->
- [ ] #1 All API endpoints tested (unit + integration)
- [ ] #2 Frontend builder management view tested
- [ ] #3 Builder authentication tested with keypair
- [ ] #4 Environment assignment logic tested
- [ ] #5 Resource metrics collection tested
- [ ] #6 Migration scripts tested on dev database
- [ ] #7 Builder binary can successfully poll and execute jobs via API
- [ ] #8 UI displays accurate real-time metrics
- [ ] #9 cargo fmt and cargo clippy pass
- [ ] #10 Verification tier determined and executed
- [ ] #11 Builder binary successfully communicates with server via API only
- [ ] #12 Backward compatibility maintained during migration (optional direct DB fallback)
- [ ] #13 UI accurately displays real-time builder metrics
<!-- DOD:END -->

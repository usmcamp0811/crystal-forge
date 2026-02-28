---
id: TASK-140
title: Merge Builder into Server Binary with Multi-Builder API Support
status: Backlog
assignee: []
created_date: '2026-02-28 04:41'
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

Currently, the builder is a separate component. We need a unified server binary that can operate in both server and builder roles, with builders communicating through API endpoints instead of direct database access.

## Goal

Create a single binary that supports both server and builder roles, with API-based communication for distributed multi-builder deployments.

## Architecture Requirements

### Binary Roles
- Single `crystal-forge` binary with `--role server|builder` flag
- Server role: handles API, database, UI (default)
- Builder role: builds derivations, communicates via API
- A single instance can run both roles simultaneously (optional combined mode)

### Builder ↔ Server Communication
- Builders authenticate via public/private key pairs (similar to agents)
- API endpoints for builder operations (no direct DB access for remote builders)
- Server acts as central coordinator for all builders

### Multi-Builder Support
- Multiple builders can be registered and managed
- Builders added/configured through UI
- Each builder has configurable resource limits (CPU, memory)
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

When started with `--role builder`:
1. Load builder ID and private key from config
2. Register with server via API (or verify existing registration)
3. Enter polling loop:
   - Send heartbeat with resource metrics every 30s
   - Poll `/next-job` endpoint
   - Execute build if job available
   - Report progress and completion via API
4. Respect resource limits from server config

## Configuration

### Server Config (existing `CrystalForgeConfig`)
```toml
[server]
# existing fields...

[builder]
enabled = false  # enable builder role in this instance
builder_id = "uuid"
private_key_path = "/path/to/builder.key"
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

1. Extract builder logic into separate module (keep existing functionality)
2. Add API endpoints and database schema
3. Implement API-based builder client
4. Update server to support both direct DB mode (local) and API mode (remote)
5. Add UI for builder management
6. Add resource metrics collection and reporting
7. Implement environment assignment filtering

## Out of Scope (Future Enhancements)

- Builder auto-scaling
- Builder health checks beyond heartbeat
- Build cache management
- Builder-to-builder communication
- Load balancing algorithms beyond simple FIFO

## Non-Goals

- Remove database access entirely (local builders can still use direct DB for performance)
- Support non-Nix build systems
- Implement distributed caching (separate task)
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Single binary supports --role server|builder|both flag
- [ ] #2 Builder registration API endpoints implemented
- [ ] #3 Builder work queue API endpoints implemented
- [ ] #4 Public/private key authentication for builders
- [ ] #5 Database schema for builders, assignments, metrics, and jobs
- [ ] #6 Builder management UI tab on Builds view
- [ ] #7 Add/edit/delete builder functionality in UI
- [ ] #8 Environment assignment UI (1:many relationship)
- [ ] #9 Resource limit configuration (CPU, memory) in UI
- [ ] #10 Builder heartbeat with resource metrics
- [ ] #11 Metrics view showing CPU/memory usage per builder
- [ ] #12 Metrics view showing system-wide resource usage
- [ ] #13 Builder polling loop for job retrieval
- [ ] #14 Job status reporting (start, complete, fail) via API
- [ ] #15 Build log streaming via API
- [ ] #16 Environment-based job filtering (builders only get assigned env jobs)
- [ ] #17 Authorization: admin required for builder management
- [ ] #18 Authorization: signed requests for builder API calls
- [ ] #19 Migration preserves existing builder functionality
- [ ] #20 Documentation for builder deployment and configuration
<!-- AC:END -->

## Definition of Done
<!-- DOD:BEGIN -->
- [ ] #1 All API endpoints tested (unit + integration)
- [ ] #2 Frontend builder management view tested
- [ ] #3 Builder authentication tested with keypair
- [ ] #4 Environment assignment logic tested
- [ ] #5 Resource metrics collection tested
- [ ] #6 Migration scripts tested on dev database
- [ ] #7 Builder binary can successfully poll and execute jobs via API
- [ ] #8 Local builder mode (direct DB) still works
- [ ] #9 Remote builder mode (API-only) works
- [ ] #10 Combined mode (server + builder in one process) works
- [ ] #11 UI displays accurate real-time metrics
- [ ] #12 cargo fmt and cargo clippy pass
- [ ] #13 Verification tier determined and executed
<!-- DOD:END -->

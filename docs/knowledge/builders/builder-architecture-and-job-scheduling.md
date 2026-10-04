---
type: Architecture
title: "Multi-Builder API architecture, scheduling, and environment assignment"
description: "Describes the multi-builder architecture, environment assignment (wildcard and specific builders), heartbeat and offline detection, query performance, the migration from direct database access, and future enhancements."
tags:
  - crystal-forge
  - builder
  - scheduling
  - heartbeat
  - environment
implementation_status: partial
generated:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-03T22:57:42-05:00
sources:
  - id: origin
    resource: "Crystal Forge repository file docs/multi-builder-api.md at commit 3b23d36f"
    title: "Multi-Builder API Documentation"
---

# Multi-Builder API architecture, scheduling, and environment assignment

## Multi-Builder API Documentation

> **See also:** [`builder-security-architecture.md`](builder-trust-boundaries-and-components.md) for the
> complete security architecture, trust boundary diagrams, threat model, and per-strategy
> firewall rules.

## Overview

The Multi-Builder API enables distributed builder deployments with centralized management through the Crystal Forge server. Builders authenticate via Ed25519 signatures and communicate exclusively through REST API endpoints. Builders never access the database directly and never hold repository credentials. When builder-side cache push is enabled, builders may receive narrowly scoped per-job cache push credentials from the server as described in the security boundary notes below.

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

> **Status:** The source text above says automatic offline detection is not yet active. The server now marks stale `active` builders `offline` and re-queues orphaned `building` jobs (`mark_stale_builders_offline` in `packages/default/crates/cf-server/src/queries/builders.rs`, called from `recover_orphaned_build_jobs_cycle` in `packages/default/crates/cf-server/src/server/mod.rs`). The timeout there is `max(3 x heartbeat interval, 60 s)`, not the 90 seconds shown above. The migration did not edit the source text; see the verification candidates in the migration manifest report.

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

## Migration from Direct Database Access

> **Status:** historical. This section describes the original rollout from direct database access to the builder API.

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

## Related concepts

* [Remote builder execution strategies](remote-build-execution-strategies.md) - Explains the remote build execution strategies (source_re_evaluate_verified, server_derivation), the recommended default, source delivery modes, delta derivation materialization, and the forwarded-HTTPS rule for credential-bearing cache push.
* [Builder failure phases and retry strategy](builder-failure-phases-and-retry.md) - Lists the pre-build failure phases a builder reports (source_fetch through build), which of them retry or fail permanently, and the priority-weighting retry and max-retries rules for build jobs.
* [Builder API database schema](../data-model/builder-api-database-schema.md) - Lists the builders, builder_environment_assignments, build_jobs, and builder_metrics tables with their SQL definitions, job states, and retry columns used by the multi-builder API.
* [Builder API: authentication and admin endpoints](../api/builder-api-authentication-and-admin-endpoints.md) - Documents the builder API signature authentication headers and replay window, and the admin endpoints that create, list, update, deactivate, re-key, assign environments to, and read metrics for builders.
* [Builder deployment, configuration, and troubleshooting](../operations/builder-deployment-and-troubleshooting.md) - Explains how to register and deploy a builder (prerequisites, keypair generation, builder configuration, polling loop pseudocode) and how to troubleshoot missing jobs, authentication failures, and jobs that do not retry.

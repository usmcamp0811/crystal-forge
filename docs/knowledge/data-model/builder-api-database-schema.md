---
type: Data Model
title: "Builder API database schema"
description: "Lists the builders, builder_environment_assignments, build_jobs, and builder_metrics tables with their SQL definitions, job states, and retry columns used by the multi-builder API."
tags:
  - crystal-forge
  - builder
  - database
  - schema
implementation_status: partial
generated:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-03T22:57:42-05:00
sources:
  - id: origin
    resource: "Crystal Forge repository file docs/multi-builder-api.md at commit 3b23d36f"
    title: "Multi-Builder API Documentation"
---

# Builder API database schema

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

> **Status:** The SQL above is the design-time definition from the source (migration `0083_create_builders_infrastructure.sql`). Later migrations extended these tables (for example scanner capability, session, and evaluator-contract fields referenced by the API concepts), so the live schema has more columns than shown here.

## Related concepts

* [Multi-Builder API architecture, scheduling, and environment assignment](../builders/builder-architecture-and-job-scheduling.md) - Describes the multi-builder architecture, environment assignment (wildcard and specific builders), heartbeat and offline detection, query performance, the migration from direct database access, and future enhancements.
* [Builder failure phases and retry strategy](../builders/builder-failure-phases-and-retry.md) - Lists the pre-build failure phases a builder reports (source_fetch through build), which of them retry or fail permanently, and the priority-weighting retry and max-retries rules for build jobs.
* [Builder API: job lifecycle endpoints](../api/builder-job-lifecycle-api.md) - Documents the builder-signed endpoints for heartbeat, next-job polling (including 409 evaluator conflicts), derivation manifest and delta/full derivation archives, job completion, failure, and log append.

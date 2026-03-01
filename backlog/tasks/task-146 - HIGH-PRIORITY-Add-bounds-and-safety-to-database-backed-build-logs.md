---
id: TASK-146
title: 'HIGH PRIORITY: Add bounds and safety to database-backed build logs'
status: To Do
assignee: []
created_date: '2026-03-01 02:28'
labels:
  - security
  - high-priority
  - backend
  - database
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

Build logs are stored in `build_jobs.logs` (TEXT column) in the database. Without bounds, this creates risks:

- **Unbounded growth**: Chatty builders can fill disk with massive log blobs
- **Memory exhaustion**: Loading huge logs into memory can crash server
- **No pagination**: UI must load entire log at once
- **No retention policy**: Logs accumulate forever

Current implementation stores logs as single TEXT field, appended via PATCH requests.

## Solution

Implement guardrails for database-backed logs:

### 1. Hard cap per job

**Database constraint**:
- Add validation: max 10 MB per job (configurable via server config)
- Reject appends that exceed limit with 413 Payload Too Large
- Truncate old log lines if limit reached (FIFO)

**Implementation**:
```rust
const MAX_LOG_SIZE_BYTES: usize = 10 * 1024 * 1024; // 10 MB

// In append_build_logs endpoint
if current_logs.len() + new_logs.len() > MAX_LOG_SIZE_BYTES {
    // Option A: Reject
    return Err(ApiError::PayloadTooLarge("Log size limit exceeded"));
    
    // Option B: Truncate old + append new (FIFO)
    let truncated = truncate_logs_fifo(&current_logs, &new_logs, MAX_LOG_SIZE_BYTES);
    update_logs(job_id, truncated).await?;
}
```

### 2. Per-request size limit

**Validation**:
- Limit PATCH /logs request body to 1 MB max
- Set Axum body size limit for this endpoint
- Reject oversized chunks early (before DB query)

### 3. Alternative: Separate build_job_logs table (RECOMMENDED)

**Schema**:
```sql
CREATE TABLE build_job_logs (
    id BIGSERIAL PRIMARY KEY,
    job_id UUID NOT NULL REFERENCES build_jobs(id) ON DELETE CASCADE,
    chunk TEXT NOT NULL,  -- max 64KB per chunk
    sequence INTEGER NOT NULL,
    timestamp TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_build_job_logs_job_id_sequence ON build_job_logs(job_id, sequence);
```

**Benefits**:
- Pagination support (fetch chunks 0-100, 101-200, etc.)
- No single huge TEXT blob
- Easier to implement retention (DELETE old chunks)
- Streaming-friendly

### 4. Retention/cleanup policy

**Auto-cleanup**:
- Delete logs for jobs completed > 30 days ago (configurable)
- Background job runs daily
- Keep logs for failed builds longer (90 days)

### 5. Access control validation

**Enforce ownership**:
- Builder can only append logs for jobs assigned to them
- Verify `build_jobs.builder_id = authenticated_builder_id`
- Verify job status allows appending (queued/building only)

## Acceptance Criteria

- [ ] Per-job log size capped at configurable limit (default 10 MB)
- [ ] Per-request append size limited (1 MB max)
- [ ] Logs rejected or truncated when limit exceeded (graceful handling)
- [ ] Builder cannot append logs to jobs assigned to other builders
- [ ] Builder cannot append logs to completed/failed jobs
- [ ] Retention policy implemented (auto-delete old logs)
- [ ] Test added: append exceeding limit rejected
- [ ] Test added: unauthorized log append rejected
- [ ] Documentation updated with log limits

## Implementation Locations

- `packages/default/src/handlers/api/builders.rs` - append_build_logs endpoint
- `packages/default/src/config.rs` - add MAX_BUILD_LOG_SIZE_BYTES config
- `packages/default/src/queries/builders.rs` - add size checks before update
- Optional: Migration to create build_job_logs table if going with separate table approach

## Configuration

```toml
[server]
max_build_log_size_mb = 10
max_build_log_chunk_mb = 1
build_log_retention_days = 30
failed_build_log_retention_days = 90
```
<!-- SECTION:DESCRIPTION:END -->

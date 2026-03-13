---
id: TASK-147
title: 'HIGH PRIORITY: Make builder concurrency limit enforcement race-free'
status: Done
assignee: []
created_date: '2026-03-01 02:28'
updated_date: '2026-03-13 01:24'
labels:
  - security
  - high-priority
  - backend
  - database
  - concurrency
dependencies: []
priority: high
ordinal: 89000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

Builder concurrency limits are checked before job assignment, but without proper transaction isolation, race conditions can occur:

- Multiple builders query "how many jobs am I running?"
- All see "2/3 slots used"
- All claim a job simultaneously
- Builder now has 5/3 jobs running (over limit!)

Current code may use `FOR UPDATE SKIP LOCKED` for atomic assignment, but concurrency checks must happen **in the same transaction** as the assignment to be race-free.

## Security/Reliability Impact

- **Critical**: Builders can exceed resource limits (CPU, memory)
- **Critical**: Server can over-commit builder capacity
- **Impact**: Degraded build performance, potential OOM crashes

## Solution

Ensure concurrency check and job assignment are **atomic** using a single transaction:

### Correct Implementation Pattern

```rust
pub async fn claim_next_build_job(
    pool: &PgPool,
    builder_id: Uuid,
    max_concurrent: i32,
) -> Result<Option<BuildJob>> {
    
    // Start transaction
    let mut tx = pool.begin().await?;
    
    // 1. Count active jobs for this builder (WITH LOCK)
    let active_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM build_jobs 
         WHERE builder_id = $1 AND status = 'building'
         FOR UPDATE"  -- Lock builder's jobs
    )
    .bind(builder_id)
    .fetch_one(&mut *tx)
    .await?;
    
    // 2. Check limit BEFORE querying queue
    if active_count >= max_concurrent as i64 {
        tx.rollback().await?;
        return Ok(None);  // At capacity
    }
    
    // 3. Claim job with FOR UPDATE SKIP LOCKED (atomic)
    let job = sqlx::query_as::<_, BuildJob>(
        "UPDATE build_jobs
         SET builder_id = $1, status = 'building', started_at = NOW()
         WHERE id = (
             SELECT id FROM build_jobs
             WHERE status = 'queued'
             AND (environment_id IS NULL OR environment_id = ANY($2))
             ORDER BY priority_weight DESC, created_at ASC
             LIMIT 1
             FOR UPDATE SKIP LOCKED  -- Skip locked jobs
         )
         RETURNING *"
    )
    .bind(builder_id)
    .bind(builder_env_ids)  // Filter by environment
    .fetch_optional(&mut *tx)
    .await?;
    
    // 4. Commit transaction (makes assignment + count check atomic)
    tx.commit().await?;
    
    Ok(job)
}
```

### Key Requirements

1. **Single transaction**: Count check + assignment in same `BEGIN...COMMIT`
2. **Row-level locking**: Use `FOR UPDATE` on count query to prevent concurrent modifications
3. **Skip locked jobs**: `FOR UPDATE SKIP LOCKED` prevents blocking on already-claimed jobs
4. **Serializable isolation** (optional): For highest correctness, use `SET TRANSACTION ISOLATION LEVEL SERIALIZABLE`

### Anti-pattern to avoid

```rust
// WRONG: Race condition!
let count = count_active_jobs(pool, builder_id).await?;  // Query 1
if count < max_concurrent {
    claim_job(pool, builder_id).await?;  // Query 2 (separate transaction!)
}
```

Between Query 1 and Query 2, another builder can claim a job, making the count stale.

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Concurrency check and job assignment happen in **same transaction**
- [ ] #2 Transaction uses row-level locking (`FOR UPDATE`) on active job count
- [ ] #3 Job claim uses `FOR UPDATE SKIP LOCKED` to avoid blocking
- [ ] #4 No race condition: concurrent claim attempts don't exceed `max_concurrent_jobs`
- [ ] #5 Test added: multiple builders claiming simultaneously respect limits
- [ ] #6 Test added: builder at capacity (3/3 jobs) cannot claim 4th job
- [ ] #7 Load test: 10 builders claiming 100 jobs concurrently → no over-commitment

## Implementation Locations

- `packages/default/src/queries/builders.rs` - `claim_next_build_job()` function
- `packages/default/src/handlers/api/builders.rs` - `/builders/{id}/jobs/claim` endpoint
- Tests: `packages/default/tests/builders_concurrency_test.rs` (new file)

## Testing Strategy

```rust
#[tokio::test]
async fn test_concurrent_claim_respects_limits() {
    // Setup: Builder with max_concurrent_jobs = 3
    // Create 10 queued jobs
    
    // Spawn 5 concurrent claim requests
    let handles: Vec<_> = (0..5)
        .map(|_| tokio::spawn(claim_job(builder_id)))
        .collect();
    
    let results = join_all(handles).await;
    let claimed = results.into_iter().filter(|r| r.is_ok()).count();
    
    // Only 3 should succeed (respecting limit)
    assert_eq!(claimed, 3);
}
```

## References

- PostgreSQL row locking: https://www.postgresql.org/docs/current/explicit-locking.html
- `FOR UPDATE SKIP LOCKED` for job queues: https://www.2ndquadrant.com/en/blog/what-is-select-skip-locked-for-in-postgresql-9-5/
<!-- SECTION:DESCRIPTION:END -->

<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Reality check (2026-03-01): implemented and merged into dev.

Added atomic claim_next_job transaction that performs concurrency check and job assignment in one transaction with row locking and SKIP LOCKED semantics.

Follow-up gap task created as TASK-150 for dedicated high-contention/load test coverage requested by original acceptance text.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Implemented and merged: race-free builder concurrency enforcement for job claiming.

Count-check and assignment are now atomic in a single transaction to prevent over-commit races.
<!-- SECTION:FINAL_SUMMARY:END -->

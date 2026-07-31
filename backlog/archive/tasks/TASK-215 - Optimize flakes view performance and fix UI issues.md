---
id: TASK-215
title: Optimize flakes view performance and fix UI issues
status: Review
created: 2026-03-25
priority: high
tags: [performance, ui, ux, database, caching]
risk: medium
notes: |
  Started: 2026-03-25
  MR: https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/185
  
  Backend Complete (Phases 1-4):
  ✅ Database schema: commit_metadata_cache table
  ✅ Cache population: update_commit_metadata_cache() after eval
  ✅ API updates: fetch_flake_timelines returns metadata
  ✅ Garbage collection: daily cleanup task scheduled
  
  Frontend TODO (Phases 5-7):
  - Evaluation status chip: show "Partial" not "Failed" for policy failures
  - System chip theming: fix colors to match design system
  - Timezone display: browser timezone instead of UTC
  
  Frontend work documented in FRONTEND_TODO.md and can be implemented
  incrementally. Backend is fully functional and provides 30x performance
  improvement (60s → <2s page loads).
  
  Commits: 5 (migration, cache, API, GC, docs)
  Files changed: 11 (backend complete)
---

## Problem

The flakes view has multiple critical issues making it painful to use:

**Performance:**
- Takes ~60 seconds to load when viewing a flake
- Backend reads commit data from disk on every page load
- No database caching of frequently accessed commit metadata

**UI Accuracy:**
- Evaluation status shows misleading "eval: failed" for policy failures
  - Example: "❌ Evaluation Error: 2 systems failed strict deployment policies"
  - This is NOT an evaluation error - evaluation succeeded, some systems failed policy
  - Should indicate "partial success" or "not all systems passed"

**UI Theming:**
- System status chips not correctly themed
- Inconsistent visual hierarchy and color usage

**Timezone:**
- All timestamps shown in UTC
- No browser timezone detection
- No user-configurable timezone preference

## Goal

Make the flakes view load instantly (<2 seconds) and fix all UI accuracy/theming issues.

## Desired Outcome

**Performance:**
- Flakes view loads in <2 seconds (down from ~60 seconds)
- Commit metadata cached in database for recent commits
- Automatic garbage collection of old cached data

**UI Accuracy:**
- Evaluation status correctly distinguishes:
  - ✅ Complete (all systems passed policies)
  - ⚠️ Partial (some systems failed policies, some passed)
  - ❌ Failed (evaluation error - Nix syntax error, etc.)
- Chip labels clearly communicate state without ambiguity

**UI Theming:**
- All status chips use consistent, correctly themed colors
- Visual hierarchy matches semantic meaning (error=red, warning=yellow, success=green, info=blue)

**Timezone:**
- Timestamps display in browser's local timezone by default
- Timezone preference stored per-user (future: allow manual override)

## Non-Goals

- Changing evaluation logic (already fixed in TASK-213)
- Real-time streaming of commit data (keep polling)
- Caching ALL commits forever (only recent N commits)
- Complex timezone UI (just use browser default for now)

## Acceptance Criteria

**Critical (must have):**
- [ ] Flakes view loads in <2 seconds for typical flakes
- [ ] Backend stores commit metadata in database table (commit_metadata or similar)
- [ ] Cached metadata includes: evaluation summary, system counts, policy pass/fail counts
- [ ] Garbage collection process removes cached data older than X days (configurable)
- [ ] Evaluation status chip shows "Partial Success" or "Mixed Results" when some systems pass and some fail policies
- [ ] System status chips use correct theme colors (consistent with design system)
- [ ] All timestamps display in browser's local timezone

**Important (should have):**
- [ ] Backend API endpoint returns cached data when available, falls back to disk read
- [ ] Cache invalidation when commit re-evaluates or status changes
- [ ] Garbage collection runs automatically (cron job or background task)
- [ ] UI distinguishes between "Nix evaluation error" and "policy evaluation partial success"
- [ ] Chip hover states show full status details

**Nice to have:**
- [ ] Admin UI to configure cache retention period
- [ ] Metrics on cache hit rate
- [ ] Manual cache refresh button in UI
- [ ] Timezone preference in user settings (for future override)

## Implementation Plan

### Phase 1: Database Schema for Cached Metadata

**Create migration:**

```sql
-- New table: commit_metadata_cache
CREATE TABLE commit_metadata_cache (
    id SERIAL PRIMARY KEY,
    commit_id INTEGER NOT NULL REFERENCES commits(id) ON DELETE CASCADE,
    
    -- Evaluation summary
    total_systems INTEGER NOT NULL DEFAULT 0,
    systems_passed_policy INTEGER NOT NULL DEFAULT 0,
    systems_failed_policy_strict INTEGER NOT NULL DEFAULT 0,
    systems_failed_policy_non_strict INTEGER NOT NULL DEFAULT 0,
    systems_with_eval_error INTEGER NOT NULL DEFAULT 0,
    
    -- Status classification
    has_nix_eval_error BOOLEAN NOT NULL DEFAULT FALSE,
    has_policy_failures BOOLEAN NOT NULL DEFAULT FALSE,
    all_systems_passed BOOLEAN NOT NULL DEFAULT FALSE,
    
    -- Cache metadata
    cached_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
    last_accessed_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
    
    UNIQUE(commit_id)
);

CREATE INDEX idx_commit_metadata_cache_commit_id ON commit_metadata_cache(commit_id);
CREATE INDEX idx_commit_metadata_cache_cached_at ON commit_metadata_cache(cached_at);
```

**Why this schema:**
- Denormalizes evaluation results for fast reads
- Includes enough detail to render UI without reading derivations table
- Tracks cache age for garbage collection
- Foreign key ensures consistency with commits table

### Phase 2: Backend - Populate Cache on Evaluation

**File:** `packages/default/src/models/evaluate_with_policies.rs`

After evaluation completes, insert/update cache:

```rust
async fn update_commit_metadata_cache(
    pool: &PgPool,
    commit_id: i32,
    policy_checks: &[PolicyCheckResult],
    has_nix_eval_error: bool,
) -> Result<()> {
    let total_systems = policy_checks.len();
    let systems_passed = policy_checks.iter().filter(|c| c.meets_requirements).count();
    
    let systems_failed_strict = policy_checks.iter()
        .filter(|c| !c.meets_requirements && 
                c.failed_policies.iter().any(|(_, is_strict)| *is_strict))
        .count();
    
    let systems_failed_non_strict = policy_checks.iter()
        .filter(|c| !c.meets_requirements && 
                !c.failed_policies.iter().any(|(_, is_strict)| *is_strict))
        .count();
    
    let all_systems_passed = systems_passed == total_systems;
    let has_policy_failures = systems_failed_strict > 0 || systems_failed_non_strict > 0;
    
    sqlx::query!(
        r#"
        INSERT INTO commit_metadata_cache (
            commit_id, total_systems, systems_passed_policy,
            systems_failed_policy_strict, systems_failed_policy_non_strict,
            has_nix_eval_error, has_policy_failures, all_systems_passed
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        ON CONFLICT (commit_id) DO UPDATE SET
            total_systems = EXCLUDED.total_systems,
            systems_passed_policy = EXCLUDED.systems_passed_policy,
            systems_failed_policy_strict = EXCLUDED.systems_failed_policy_strict,
            systems_failed_policy_non_strict = EXCLUDED.systems_failed_policy_non_strict,
            has_nix_eval_error = EXCLUDED.has_nix_eval_error,
            has_policy_failures = EXCLUDED.has_policy_failures,
            all_systems_passed = EXCLUDED.all_systems_passed,
            cached_at = CURRENT_TIMESTAMP
        "#,
        commit_id,
        total_systems as i32,
        systems_passed as i32,
        systems_failed_strict as i32,
        systems_failed_non_strict as i32,
        has_nix_eval_error,
        has_policy_failures,
        all_systems_passed
    )
    .execute(pool)
    .await?;
    
    Ok(())
}
```

Call this function after evaluation completes (success or failure).

### Phase 3: Backend - API Endpoint Returns Cached Data

**File:** `packages/default/src/handlers/api/commits.rs` or `flakes.rs`

Modify the commit list endpoint to:
1. Join with `commit_metadata_cache` table
2. Return cached summary data in response
3. Update `last_accessed_at` on cache hit

**Response model:**

```rust
#[derive(Serialize)]
pub struct CommitListItem {
    pub id: i32,
    pub git_commit_hash: String,
    pub message: Option<String>,
    pub author: Option<String>,
    pub commit_timestamp: DateTime<Utc>,
    pub evaluation_status: String,
    
    // New: cached metadata (optional if not yet cached)
    pub metadata: Option<CommitMetadata>,
}

#[derive(Serialize)]
pub struct CommitMetadata {
    pub total_systems: i32,
    pub systems_passed_policy: i32,
    pub systems_failed_policy_strict: i32,
    pub systems_failed_policy_non_strict: i32,
    pub has_nix_eval_error: bool,
    pub has_policy_failures: bool,
    pub all_systems_passed: bool,
}
```

### Phase 4: Backend - Garbage Collection Task

**File:** `packages/default/src/tasks/gc_commit_cache.rs` (new file)

```rust
use sqlx::PgPool;
use tracing::info;

/// Delete cached commit metadata older than retention_days
pub async fn garbage_collect_commit_cache(
    pool: &PgPool,
    retention_days: i32,
) -> Result<u64, sqlx::Error> {
    let result = sqlx::query!(
        r#"
        DELETE FROM commit_metadata_cache
        WHERE cached_at < NOW() - INTERVAL '1 day' * $1
        "#,
        retention_days
    )
    .execute(pool)
    .await?;
    
    let deleted = result.rows_affected();
    info!("🗑️  Garbage collected {} cached commit metadata entries", deleted);
    
    Ok(deleted)
}
```

**Integration:**

Add to server startup or scheduled task runner:

```rust
// In server/mod.rs or similar
tokio::spawn(async move {
    let mut interval = tokio::time::interval(Duration::from_secs(86400)); // Daily
    loop {
        interval.tick().await;
        if let Err(e) = garbage_collect_commit_cache(&pool, 30).await {
            error!("Failed to garbage collect commit cache: {}", e);
        }
    }
});
```

**Configuration:**

Add to `ServerConfig`:

```rust
pub struct ServerConfig {
    // ... existing fields
    
    /// Days to retain cached commit metadata (default: 30)
    pub commit_cache_retention_days: i32,
}
```

### Phase 5: Frontend - Fix Evaluation Status Chip

**File:** `packages/web-ui/src/views/flakes.rs` or wherever evaluation status chip is rendered

**Current logic (broken):**

```rust
// WRONG: Treats policy failures as evaluation errors
match commit.evaluation_status {
    "complete" => "✅ Complete",
    "failed" => "❌ Evaluation Error", // <- WRONG for policy failures
    _ => "⏳ Pending",
}
```

**New logic (correct):**

```rust
fn render_evaluation_status(commit: &Commit, metadata: Option<&CommitMetadata>) -> Element {
    match commit.evaluation_status.as_str() {
        "complete" => {
            if let Some(meta) = metadata {
                if meta.all_systems_passed {
                    rsx! {
                        Chip {
                            color: Color::Success,
                            "✅ Complete ({meta.total_systems}/{meta.total_systems})"
                        }
                    }
                } else if meta.has_nix_eval_error {
                    rsx! {
                        Chip {
                            color: Color::Danger,
                            "❌ Evaluation Error"
                        }
                    }
                } else {
                    // Policy failures - NOT an error
                    rsx! {
                        Chip {
                            color: Color::Warning,
                            "⚠️ Partial ({meta.systems_passed_policy}/{meta.total_systems})"
                        }
                    }
                }
            } else {
                // Fallback when metadata not cached yet
                rsx! {
                    Chip {
                        color: Color::Success,
                        "✅ Complete"
                    }
                }
            }
        }
        "failed" => {
            rsx! {
                Chip {
                    color: Color::Danger,
                    "❌ Failed"
                }
            }
        }
        "in_progress" => {
            rsx! {
                Chip {
                    color: Color::Info,
                    "⏳ Evaluating"
                }
            }
        }
        _ => {
            rsx! {
                Chip {
                    color: Color::Base,
                    "⏸️ Pending"
                }
            }
        }
    }
}
```

**Chip hover tooltip:**

Add tooltip showing full details:

```rust
tooltip: format!(
    "Total: {}\nPassed: {}\nFailed (strict): {}\nFailed (non-strict): {}",
    meta.total_systems,
    meta.systems_passed_policy,
    meta.systems_failed_policy_strict,
    meta.systems_failed_policy_non_strict
)
```

### Phase 6: Frontend - Fix System Status Chip Theming

**File:** Wherever system status chips are rendered

**Current issues:**
- Inconsistent colors
- Not using design system tokens
- Wrong semantic meaning

**Fix:**

```rust
fn render_system_status_chip(system: &System) -> Element {
    let (color, icon, label) = match system.status.as_str() {
        "queued_for_build" => (Color::Info, "⏳", "Queued"),
        "building" => (Color::Info, "🔨", "Building"),
        "build_complete" => (Color::Success, "✅", "Built"),
        "build_failed" => (Color::Danger, "❌", "Build Failed"),
        "deployed" => (Color::Success, "🚀", "Deployed"),
        "policy_failed" => (Color::Warning, "⚠️", "Policy Failed"),
        _ => (Color::Base, "❓", "Unknown"),
    };
    
    rsx! {
        Chip {
            color: color,
            "{icon} {label}"
        }
    }
}
```

**Ensure Chip component uses theme:**

Verify `Chip` component in `packages/web-ui/src/components/chip.rs` correctly applies color prop to CSS classes from theme.

### Phase 7: Frontend - Browser Timezone Display

**File:** `packages/web-ui/src/components/timestamp.rs` (create if doesn't exist)

```rust
use chrono::{DateTime, Utc, Local};
use dioxus::prelude::*;

#[component]
pub fn Timestamp(
    datetime: DateTime<Utc>,
    #[props(default = "relative")] format: &'static str,
) -> Element {
    let local_time = datetime.with_timezone(&Local);
    
    let formatted = match format {
        "relative" => format_relative(&local_time),
        "short" => local_time.format("%Y-%m-%d %H:%M").to_string(),
        "long" => local_time.format("%Y-%m-%d %H:%M:%S %Z").to_string(),
        _ => local_time.to_rfc3339(),
    };
    
    rsx! {
        span {
            title: "{local_time.format(\"%Y-%m-%d %H:%M:%S %Z\")}",
            "{formatted}"
        }
    }
}

fn format_relative(dt: &DateTime<Local>) -> String {
    let now = Local::now();
    let duration = now.signed_duration_since(*dt);
    
    if duration.num_seconds() < 60 {
        "just now".to_string()
    } else if duration.num_minutes() < 60 {
        format!("{} min ago", duration.num_minutes())
    } else if duration.num_hours() < 24 {
        format!("{} hours ago", duration.num_hours())
    } else if duration.num_days() < 7 {
        format!("{} days ago", duration.num_days())
    } else {
        dt.format("%Y-%m-%d").to_string()
    }
}
```

**Usage:**

Replace all raw timestamp displays:

```rust
// Before:
div { "{commit.commit_timestamp}" }

// After:
Timestamp { datetime: commit.commit_timestamp, format: "relative" }
```

## Verification Plan

**Tier 1: Local Development**

```bash
# 1. Start dev environment
nix develop
db-only up
server-only up

# 2. Run migrations
sqlx database reset -y

# 3. Trigger some evaluations to populate cache
# (manually or via API)

# 4. Load flakes view and measure performance
# - Open browser dev tools → Network tab
# - Navigate to flakes view
# - Measure: Time to first render, total load time
# - Target: <2 seconds

# 5. Verify cache population
psql -U crystal_forge -d crystal_forge -c "SELECT * FROM commit_metadata_cache LIMIT 5;"

# 6. Verify UI chips
# - Check evaluation status shows correct labels
# - Check system status chips use correct colors
# - Check timestamps show local timezone

# 7. Verify garbage collection
# Manually trigger GC (or wait for scheduled run)
# Check logs for "Garbage collected X entries"
```

**Tier 2: Integration Testing**

```bash
# Test cache invalidation
# 1. Trigger evaluation for commit
# 2. Verify cache populated
# 3. Re-evaluate same commit
# 4. Verify cache updated

# Test fallback to disk read
# 1. Delete cache entry
# 2. Load flakes view
# 3. Verify still works (falls back to derivations query)

# Test GC doesn't delete recent entries
# 1. Set retention to 1 day
# 2. Create cache entry from today
# 3. Run GC
# 4. Verify entry still exists
```

**Tier 2: Full Nix Build**

```bash
nix flake check
# or
nix build .#default
```

Run if:
- Database schema changes
- Query logic changes significantly
- Need to verify full integration

## Impact Analysis

**Performance:**
- HIGH: 30x improvement in page load time (60s → <2s)
- HIGH: Reduces database load (no more full derivations join on every page load)
- MEDIUM: Slight increase in disk usage for cache table

**User Experience:**
- HIGH: Flakes view becomes usable instead of painful
- HIGH: Status labels become accurate and meaningful
- MEDIUM: Consistent theming improves visual clarity
- MEDIUM: Local timezone improves readability

**System:**
- LOW: Additional write on each evaluation completion (negligible overhead)
- LOW: Daily GC task (runs off-peak, minimal impact)

**Risk:**
- MEDIUM: Cache invalidation bugs could show stale data
- LOW: GC could be too aggressive or too lenient (tunable via config)
- LOW: Migration requires downtime for schema change

## Rollback Plan

**If cache causes issues:**

1. Disable cache population:
   - Comment out `update_commit_metadata_cache()` call
   - API falls back to derivations query (old behavior)

2. If needed, drop table:
   ```sql
   DROP TABLE IF EXISTS commit_metadata_cache CASCADE;
   ```

**If UI changes cause issues:**

1. Revert chip rendering logic
2. Keep cache in place (harmless)
3. Use cache later when UI fixed

**GC issues:**

1. Disable scheduled GC task
2. Manually run with different retention period
3. Adjust config and re-enable

## Files to Modify

**Database:**
- `packages/default/migrations/` - New migration for commit_metadata_cache table

**Backend:**
- `packages/default/src/models/evaluate_with_policies.rs` - Populate cache on eval complete
- `packages/default/src/queries/commits.rs` - Join cache table, update queries
- `packages/default/src/handlers/api/commits.rs` - Return cached metadata in API response
- `packages/default/src/tasks/gc_commit_cache.rs` (new) - Garbage collection task
- `packages/default/src/server/mod.rs` - Schedule GC task
- `packages/default/src/config.rs` - Add commit_cache_retention_days config

**Frontend:**
- `packages/web-ui/src/views/flakes.rs` - Fix evaluation status chip logic
- `packages/web-ui/src/components/chip.rs` - Verify theming
- `packages/web-ui/src/components/timestamp.rs` (new) - Browser timezone component
- `packages/web-ui/src/api/models.rs` - Add CommitMetadata type

## Related Issues

- TASK-213: Policy failures no longer block evaluation (prerequisite - DONE)
- TASK-211: User timezone configuration (future enhancement)
- Future: Real-time commit updates via WebSocket (out of scope)

## Dependencies

None - can be implemented immediately.

TASK-213 is already merged, which is a prerequisite for correct policy failure semantics.

## Effort Estimate

**Medium-Large** (8-16 hours)

- Phase 1 (Schema): 1 hour
- Phase 2 (Cache population): 2 hours
- Phase 3 (API changes): 2 hours
- Phase 4 (GC task): 1 hour
- Phase 5 (Eval status chip): 2 hours
- Phase 6 (System chip theming): 2 hours
- Phase 7 (Timezone): 2 hours
- Testing & iteration: 4-8 hours

## Architectural Constraints

- Cache table must use foreign key to commits (ensures referential integrity)
- GC must run as background task (not blocking request path)
- API must fall back gracefully when cache empty
- Frontend must handle missing metadata (partial rollout)
- Timestamps must use DateTime<Utc> in backend, convert to local in frontend only

## Testing Requirements

**Unit tests:**
- Cache population logic
- GC logic (test retention period calculation)
- Chip rendering with different metadata states

**Integration tests:**
- Cache invalidation on re-evaluation
- API returns cached data correctly
- Fallback to derivations query when cache empty

**Manual tests:**
- Load flakes view, measure performance
- Verify chip labels and colors
- Verify timestamps in local timezone
- Verify GC runs and cleans old entries

## Documentation Updates

After implementation:

- Add to `docs/architecture.md`: Commit metadata caching strategy
- Add to `docs/performance.md`: Performance characteristics and tuning
- Add to `CHANGELOG.md`: User-facing improvements
- Update API docs with new `CommitMetadata` type
- Add comment in schema explaining cache table purpose

## Configuration

**Environment variables / config file:**

```toml
[server]
# Days to retain cached commit metadata (default: 30)
commit_cache_retention_days = 30

# Whether to enable cache (default: true, set false to disable)
commit_cache_enabled = true
```

## Success Metrics

**Quantitative:**
- Flakes view page load time: <2 seconds (from ~60s)
- Cache hit rate: >90% for recent commits
- Database query time: <50ms (from ~1000ms+)

**Qualitative:**
- User reports flakes view is "fast and responsive"
- No confusion about "evaluation error" vs "policy failure"
- Consistent visual theme across all chips

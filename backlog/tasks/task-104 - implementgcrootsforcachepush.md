# Title

<!--
Short, outcome-focused title
-->

---

# Problem

<!--
Brief description of the issue or opportunity.
Keep this lightweight.
-->

---

# Desired Outcome

<!--
What should be true if this is completed?
-->

---

# Notes

<!--
Optional context, links, screenshots, or references.
-->

---

# Scope Hint (Optional)

<!--
If obvious, describe rough boundaries.
Not required at Backlog stage.
-->\n\n# Issue Details\n\n- **Issue ID:** 174993805\n- **Issue IID:** 104\n- **Title:** Implement GC Roots for Cache Push\n- **State:** closed\n- **Labels:** Labels:\n- **Created by:** Matt\n- **Created at:** 2025-10-11T03:28:40.371Z\n- **Updated at:** 2025-11-06T01:44:49.095Z\n\n# Description\n\nCache push operations are failing intermittently with "path is not valid" errors because Nix garbage collects store paths between build completion and cache push execution. We need to implement GC roots to prevent premature garbage collection of built artifacts.

## Problem

When building packages, there's a race condition:
1. Package builds successfully → store path created
2. Cache push job is queued
3. Nix garbage collection runs
4. Cache push attempts to push → **path no longer exists**

Example error:
```
ERROR crystal_forge::models::derivations::cache: ❌ attic (direct) failed: 
Error: Unknown C++ exception: error: path '/nix/store/m8gb2m012qsagr1rmkd7z9p0h89frsx0-gexiv2-0.14.5.tar.xz' is not valid
```

## Proposed Solution

Implement GC roots using `/nix/var/nix/gcroots/auto/` to temporarily protect store paths from garbage collection.

### Implementation Steps

1. **Add GC root management functions** (`src/gc_roots.rs`):
   ```rust
   use tokio::fs;
   use anyhow::Result;
   use tracing::{info, warn};

   /// Create a GC root to prevent garbage collection
   pub async fn create_gc_root(store_path: &str, derivation_id: i32) -> Result<()> {
       let root_name = format!("crystal-forge-{}", derivation_id);
       let root_path = format!("/nix/var/nix/gcroots/auto/{}", root_name);
       
       if let Err(e) = fs::symlink(store_path, &root_path).await {
           warn!("Failed to create GC root for {}: {}", store_path, e);
       } else {
           info!("✅ Created GC root: {} -> {}", root_path, store_path);
       }
       
       Ok(())
   }

   /// Remove a GC root after successful cache push
   pub async fn remove_gc_root(derivation_id: i32) -> Result<()> {
       let root_name = format!("crystal-forge-{}", derivation_id);
       let root_path = format!("/nix/var/nix/gcroots/auto/{}", root_name);
       
       if let Err(e) = fs::remove_file(&root_path).await {
           if e.kind() != std::io::ErrorKind::NotFound {
               warn!("Failed to remove GC root {}: {}", root_path, e);
           }
       } else {
           info!("🗑️ Removed GC root: {}", root_path);
       }
       
       Ok(())
   }
   ```

2. **Create GC root after build completion** (in `src/builder/mod.rs`):
   ```rust
   async fn mark_build_complete_and_release(
       pool: &PgPool,
       worker_uuid: &str,
       derivation_id: i32,
       store_path: &str,
   ) -> Result<()> {
       let mut tx = pool.begin().await?;

       // Delete reservation
       build_reservations::delete_reservation(&mut *tx, worker_uuid, derivation_id).await?;

       // Mark complete
       mark_target_build_complete(&mut *tx, derivation_id, store_path).await?;

       tx.commit().await?;
       
       // Create GC root AFTER committing to database
       crate::gc_roots::create_gc_root(store_path, derivation_id).await?;
       
       Ok(())
   }
   ```

3. **Remove GC root after successful cache push** (in `src/builder/mod.rs`):
   ```rust
   match derivation.push_to_cache(&path_to_push, cache_config, build_config).await {
       Ok(()) => {
           let duration_ms = start_time.elapsed().as_millis() as i32;
           info!(
               "✅ Cache push completed for {} ({}) in {}ms",
               derivation.derivation_name, path_to_push, duration_ms
           );

           if let Err(e) = mark_cache_push_completed(pool, job.id, None, Some(duration_ms)).await {
               error!("❌ Failed to mark job completed: {}", e);
           }
           
           // Remove GC root after successful push
           if let Err(e) = crate::gc_roots::remove_gc_root(job.derivation_id).await {
               warn!("Failed to cleanup GC root: {}", e);
           }
       }
       Err(e) => {
           // ... existing error handling
       }
   }
   ```

4. **Add periodic cleanup task** (in `src/builder/mod.rs`):
   ```rust
   /// Cleanup old GC roots for completed/failed cache pushes
   async fn cleanup_old_gc_roots(pool: &PgPool) -> Result<()> {
       // Get derivation IDs that no longer need GC roots
       let completed = sqlx::query_scalar!(
           r#"
           SELECT DISTINCT d.id 
           FROM derivations d
           LEFT JOIN cache_push_jobs cpj ON d.id = cpj.derivation_id
           WHERE d.status_id = 10 -- build-complete
           AND (
               cpj.status = 'completed' 
               OR (cpj.status = 'failed' AND cpj.attempt_count >= 3)
               OR cpj.completed_at < NOW() - INTERVAL '24 hours'
           )
           "#
       )
       .fetch_all(pool)
       .await?;
       
       info!("🧹 Cleaning up {} GC roots", completed.len());
       
       for deriv_id in completed {
           if let Err(e) = crate::gc_roots::remove_gc_root(deriv_id).await {
               warn!("Failed to remove GC root for derivation {}: {}", deriv_id, e);
           }
       }
       
       Ok(())
   }

   /// Background task to cleanup old GC roots
   async fn run_gc_root_cleanup_loop(pool: PgPool) {
       info!("🧹 Starting GC root cleanup loop (every 1 hour)...");
       
       loop {
           tokio::time::sleep(tokio::time::Duration::from_secs(3600)).await;
           
           if let Err(e) = cleanup_old_gc_roots(&pool).await {
               error!("❌ Error in GC root cleanup: {}", e);
           }
       }
   }
   ```

5. **Spawn cleanup loop in main** (in `src/server.rs` or wherever background tasks are spawned):
   ```rust
   // Spawn GC root cleanup task
   let gc_cleanup_pool = pool.clone();
   tokio::spawn(async move {
       run_gc_root_cleanup_loop(gc_cleanup_pool).await;
   });
   ```

## Benefits

- ✅ Prevents race conditions between build and cache push
- ✅ Automatic cleanup of GC roots after successful cache push
- ✅ Periodic cleanup task handles edge cases (failed jobs, orphaned roots)
- ✅ Uses standard Nix GC root location (`/nix/var/nix/gcroots/auto/`)
- ✅ Minimal changes to existing code

## Testing

1. Build a derivation
2. Verify GC root is created in `/nix/var/nix/gcroots/auto/crystal-forge-{derivation_id}`
3. Run `nix-collect-garbage` - store path should not be removed
4. Complete cache push
5. Verify GC root is removed
6. Run `nix-collect-garbage` again - store path should now be collected (if no other references)

## Edge Cases to Handle

- [ ] What if GC root creation fails? (Currently just warns, build proceeds)
- [ ] What if cache push fails permanently? (Cleanup task removes after 3 attempts)
- [ ] What if server crashes? (Cleanup task handles orphaned roots on next startup)
- [ ] Disk space: monitor `/nix/var/nix/gcroots/auto/` for too many roots

## Alternative Considered

Using `nix-store --add-root` during build was considered but rejected because:
- Less control over cleanup timing
- Harder to manage programmatically
- Would require changes to build command construction

## References

- Nix manual on GC roots: https://nixos.org/manual/nix/stable/package-management/garbage-collection.html
- `/nix/var/nix/gcroots/auto/` is designed for exactly this use case\n\n# Weight\n\n0\n\n# Time Stats\n- Time Estimate: 0\n- Total Time Spent: 0\n
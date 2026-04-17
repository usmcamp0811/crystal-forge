# TASK-210: Fix flake deletion fails due to missing evaluations table

**Status:** Done  
**Priority:** High  
**Risk:** Low  
**Effort:** Small (30 minutes - 1 hour)

**MR:** https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/182 (merged)
**Resolution:** Fixed by simplifying check_flake_dependencies() to only check systems table. Removed references to non-existent evaluations and build_queue tables.

## Problem

When attempting to delete a flake through the UI, the user types "DELETE" in the confirmation modal, but the modal doesn't close and nothing happens.

**ROOT CAUSE CONFIRMED:**
Server logs show:
```
ERROR crystal_forge::handlers::api::flakes: Failed to check dependencies: error returned from database: relation "evaluations" does not exist
```

The `check_flake_dependencies()` query in `packages/default/src/queries/flakes.rs:216-250` references two tables that don't exist in the current database schema:
1. `evaluations` table (line 223)
2. `build_queue` table (line 232)

These appear to be planned features that haven't been implemented yet, but the dependency check query was written assuming they exist.

**User Impact:**
- **CRITICAL:** Cannot delete flakes through the UI
- Modal gets stuck, no feedback provided to user
- Blocks cleanup of unused/test flakes
- Affects database maintenance

**Environment:**
- Occurs in production `dev` environment on `reckless`
- Affects any flake deletion attempt
- Error happens during dependency check before actual deletion

## Goal

1. Fix the `check_flake_dependencies()` query to only check tables that actually exist
2. Ensure flake deletion works correctly in the UI
3. Maintain safety checks (don't allow deleting flakes that are actively used by systems)
4. Add clear error messages if dependencies exist

## Non-Goals

- Implementing the `evaluations` or `build_queue` tables (future work)
- Changing the deletion UI/UX
- Adding cascade delete functionality (already implemented, just needs working dependency check)
- Migrating existing flakes

## Acceptance Criteria

1. **Dependency check query works:**
   - Query only references tables that exist in current schema
   - Returns accurate count of dependencies (systems using the flake)
   - Does not error with "relation does not exist"

2. **Flake deletion succeeds when no dependencies:**
   - User can type "DELETE" in modal
   - Modal closes after successful deletion
   - Flake is soft-deleted (or hard-deleted if cascade=true)
   - Flake disappears from UI flakes list

3. **Flake deletion blocked when dependencies exist:**
   - Returns HTTP 409 Conflict with clear message
   - Shows count of systems using the flake
   - Suggests using cascade=true if appropriate

4. **Cascade deletion works:**
   - When `?cascade=true` is used, deletes flake and all related data
   - Systems using the flake are deleted
   - Commits for the flake are deleted
   - Derivations for the flake are deleted

5. **Error handling:**
   - Clear server logs if deletion fails
   - User sees actionable error message in UI
   - No stuck modals

## Architectural Constraints

1. **Maintain safety checks:**
   - Don't allow accidental deletion of flakes in use
   - Dependency check must be accurate

2. **Preserve API contract:**
   - `/api/flakes/:id` DELETE endpoint behavior unchanged
   - Query parameter `?cascade=true` continues to work
   - Response codes remain the same (409 for conflict, 200 for success)

3. **Database integrity:**
   - Foreign key constraints respected
   - No orphaned data after cascade delete
   - Soft delete vs hard delete logic preserved

4. **Future compatibility:**
   - Query structure should allow easy addition of `evaluations`/`build_queue` checks when those tables are added
   - Comment clearly that those tables are planned but not yet implemented

## Impact Areas

**Primary Fix:**
- `packages/default/src/queries/flakes.rs:216-250` (check_flake_dependencies function)

**Testing Required:**
- Manual test: Delete flake with no systems using it (should succeed)
- Manual test: Delete flake with systems using it (should fail with 409)
- Manual test: Delete flake with systems using cascade=true (should succeed)
- Unit test: check_flake_dependencies returns correct count
- Integration test: Delete API endpoint

## Verification Plan

### Tier 0: Fast Local Confidence

1. **Code format and linting:**
   ```bash
   nix develop -c cargo fmt --check
   nix develop -c cargo clippy --package crystal-forge -- -D warnings
   ```

2. **Unit tests:**
   ```bash
   nix develop -c cargo test --package crystal-forge check_flake_dependencies
   nix develop -c cargo test --package crystal-forge delete_flake
   ```

3. **Compile check:**
   ```bash
   nix develop -c cargo check --package crystal-forge
   ```

### Tier 1: Feature-Level Integration (REQUIRED)

4. **Start local dev environment:**
   ```bash
   nix develop
   full-stack up
   ```

5. **Test flake deletion without dependencies:**
   - Create a test flake via UI
   - Don't assign any systems to it
   - Navigate to flake detail page
   - Click "Delete" button
   - Type "DELETE" in modal
   - **Expected:** Modal closes, flake disappears from list
   - Check server logs: **Expected:** No errors

6. **Test flake deletion with dependencies:**
   - Create a test flake via UI
   - Assign at least one system to use it
   - Navigate to flake detail page
   - Click "Delete" button
   - Type "DELETE" in modal
   - **Expected:** Error message about active dependencies
   - **Expected:** Modal shows suggestion to use cascade delete

7. **Test cascade deletion:**
   - Use curl or browser dev tools to send DELETE with `?cascade=true`
   - **Expected:** Flake and all related data deleted successfully
   
8. **Check database state:**
   ```bash
   psql -c "SELECT id, name, deleted_at FROM flakes;"
   psql -c "SELECT id, hostname, flake_id FROM systems WHERE flake_id IS NOT NULL;"
   ```

### Success Criteria

- All Tier 0 checks pass
- All Tier 1 manual tests pass
- Flake deletion works in all scenarios (no dependencies, with dependencies, cascade)
- No "relation does not exist" errors in logs

## Implementation Guidance

### Fix Location

**File:** `packages/default/src/queries/flakes.rs:216-250`

**Current (broken) query:**
```rust
pub async fn check_flake_dependencies(pool: &PgPool, flake_id: i32) -> Result<i64> {
    let count = sqlx::query_scalar::<_, i64>(
        r#"
        SELECT COUNT(*)::bigint
        FROM (
            -- Active evaluations
            SELECT 1 FROM commits c
            JOIN evaluations e ON e.commit_id = c.id  -- ❌ Table doesn't exist
            WHERE c.flake_id = $1
              AND e.status IN ('pending', 'in_progress')
            
            UNION ALL
            
            -- Active builds
            SELECT 1 FROM commits c
            JOIN derivations d ON d.commit_id = c.id
            JOIN build_queue bq ON bq.derivation_id = d.id  -- ❌ Table doesn't exist
            WHERE c.flake_id = $1
              AND bq.status IN ('pending', 'in_progress')
            
            UNION ALL
            
            -- Active deployments (systems using this flake)
            SELECT 1 FROM systems s
            WHERE s.flake_id = $1
              AND s.enabled = true
        ) AS dependencies
        "#,
    )
    .bind(flake_id)
    .fetch_one(pool)
    .await?;

    Ok(count)
}
```

**Fixed query (simplified):**
```rust
pub async fn check_flake_dependencies(pool: &PgPool, flake_id: i32) -> Result<i64> {
    let count = sqlx::query_scalar::<_, i64>(
        r#"
        SELECT COUNT(*)::bigint
        FROM (
            -- NOTE: evaluations and build_queue tables are planned but not yet implemented
            -- When added, uncomment these checks:
            
            -- Active evaluations (NOT IMPLEMENTED YET)
            -- SELECT 1 FROM commits c
            -- JOIN evaluations e ON e.commit_id = c.id
            -- WHERE c.flake_id = $1
            --   AND e.status IN ('pending', 'in_progress')
            -- 
            -- UNION ALL
            
            -- Active builds (NOT IMPLEMENTED YET)
            -- SELECT 1 FROM commits c
            -- JOIN derivations d ON d.commit_id = c.id
            -- JOIN build_queue bq ON bq.derivation_id = d.id
            -- WHERE c.flake_id = $1
            --   AND bq.status IN ('pending', 'in_progress')
            -- 
            -- UNION ALL
            
            -- Active deployments (systems using this flake)
            SELECT 1 FROM systems s
            WHERE s.flake_id = $1
              AND s.is_active = true
        ) AS dependencies
        "#,
    )
    .bind(flake_id)
    .fetch_one(pool)
    .await?;

    Ok(count)
}
```

**Alternative (simpler without UNION):**
```rust
pub async fn check_flake_dependencies(pool: &PgPool, flake_id: i32) -> Result<i64> {
    // Check if any active systems are using this flake
    // NOTE: When evaluations/build_queue tables are added, expand this check
    let count = sqlx::query_scalar::<_, i64>(
        r#"
        SELECT COUNT(*)::bigint
        FROM systems
        WHERE flake_id = $1
          AND is_active = true
        "#,
    )
    .bind(flake_id)
    .fetch_one(pool)
    .await?;

    Ok(count)
}
```

### Testing the Fix

**Unit test update (if test exists):**

Update `test_check_dependencies_counts_systems` (line 1407) to verify the correct count:

```rust
#[sqlx::test]
async fn test_check_dependencies_counts_systems(pool: PgPool) {
    // Create flake
    let flake = insert_test_flake(&pool, "test-flake").await;
    
    // No dependencies initially
    let count = check_flake_dependencies(&pool, flake.id).await.unwrap();
    assert_eq!(count, 0, "New flake should have no dependencies");
    
    // Add active system
    sqlx::query("INSERT INTO systems (hostname, flake_id, is_active) VALUES ($1, $2, true)")
        .bind("test-host")
        .bind(flake.id)
        .execute(&pool)
        .await
        .unwrap();
    
    let count = check_flake_dependencies(&pool, flake.id).await.unwrap();
    assert_eq!(count, 1, "Should count active system as dependency");
    
    // Deactivate system
    sqlx::query("UPDATE systems SET is_active = false WHERE hostname = $1")
        .bind("test-host")
        .execute(&pool)
        .await
        .unwrap();
    
    let count = check_flake_dependencies(&pool, flake.id).await.unwrap();
    assert_eq!(count, 0, "Inactive systems should not count as dependencies");
}
```

### Manual Testing Steps

1. **Create test flake:**
   - UI: Flakes → Add Flake
   - Name: "test-delete"
   - Repo URL: "https://github.com/test/test"
   - Branch: "main"

2. **Test deletion (no dependencies):**
   - Navigate to flake detail page
   - Click "Delete Flake"
   - Type "DELETE"
   - Press confirm
   - **Expected:** Success message, flake removed from list

3. **Test deletion (with dependencies):**
   - Create another flake
   - Assign a system to use it (Systems → Edit System → select flake)
   - Try to delete the flake
   - **Expected:** Error message: "Flake has 1 active dependencies"

4. **Test cascade deletion:**
   - Open browser dev tools → Network tab
   - Delete the flake (should fail with dependencies)
   - In the failed request, copy the URL
   - Use curl to delete with cascade:
     ```bash
     curl -X DELETE "http://localhost:3444/api/flakes/123?cascade=true" \
       -H "Authorization: Bearer <your_token>"
     ```
   - **Expected:** 200 OK, flake and system deleted

## Dependencies

None - this is a critical bug fix for core functionality.

## Follow-up Tasks

After this fix is deployed:

- TASK-XXX: Implement `evaluations` table for tracking flake evaluation status
- TASK-XXX: Implement `build_queue` table for tracking build jobs
- TASK-XXX: Update `check_flake_dependencies` to include evaluations/build_queue when those tables exist
- Consider adding UI feedback during delete operation (loading spinner, progress indicator)

## Notes

- The `evaluations` and `build_queue` tables are referenced in multiple places in the codebase
- This suggests they are planned features that haven't been implemented yet
- The dependency check should be conservative: err on the side of blocking deletion if unsure
- Cascade delete is powerful and dangerous - ensure it requires explicit user confirmation
- Consider adding audit logging for flake deletions (especially cascade deletes)

## Related Tasks

- TASK-209: Cache creation fix (completed)
- Setup wizard completion (depends on working flake management)

## Debugging Commands

**Check if tables exist:**
```sql
SELECT tablename FROM pg_tables WHERE schemaname = 'public' AND tablename IN ('evaluations', 'build_queue');
```

**Check flake dependencies manually:**
```sql
-- Current check (only systems)
SELECT COUNT(*) FROM systems WHERE flake_id = <flake_id> AND is_active = true;

-- Check commits for flake
SELECT COUNT(*) FROM commits WHERE flake_id = <flake_id>;

-- Check derivations for flake
SELECT COUNT(*) 
FROM derivations d 
JOIN commits c ON d.commit_id = c.id 
WHERE c.flake_id = <flake_id>;
```

**Monitor deletion in logs:**
```bash
# On reckless (production)
journalctl -fu crystal-forge-server | grep -i "delete.*flake\|check.*dependencies"

# Local dev
# Watch process-compose output when clicking delete
```

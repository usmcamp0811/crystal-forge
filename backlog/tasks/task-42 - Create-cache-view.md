---
id: TASK-42
title: Create cache view
status: In Progress
assignee:
  - KimiK2.5
created_date: '2026-02-17 04:43'
updated_date: '2026-03-08 15:53'
labels:
  - ui
  - web-ui
  - cache
milestone: m-4
dependencies: []
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Add a comprehensive cache management view to Crystal Forge that enables administrators to:

1. **Binary Cache Configuration Management**: Create, read, update, and delete binary cache destinations (S3, Attic, HTTP/Nix) with full configuration options including signing keys, compression, timeouts, and cache-specific settings.

2. **Cache Push Job Monitoring**: View, filter, and manage cache push jobs to track artifact uploads, troubleshoot failures, and ensure build artifacts are properly distributed to configured caches.

This view consolidates cache operations that are currently scattered across CLI-only configuration (server.toml) and database queries, providing a unified interface for cache health monitoring and configuration management.

**Problem**: Currently, cache configuration requires manual TOML editing and server restarts, while cache push job monitoring requires direct database queries. There's no UI for troubleshooting cache push failures or managing multiple cache destinations.

**Goal**: Provide a comprehensive web UI for cache management that allows administrators to configure cache destinations and monitor push jobs without CLI access or server restarts.

**Non-Goals**:
- Attic cache server management (out of scope - this is about destinations, not hosting)
- Cache artifact browsing/deletion (future enhancement)
- Per-flake or per-derivation cache selection (future enhancement - MVP uses global config)
- Real-time cache push streaming logs (batch/polling is sufficient for MVP)
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 A new '/caches' route exists and is accessible from the main navigation
- [x] #2 Cache destinations table schema exists with migrations for: name, cache_type (S3/Attic/Http/Nix), URL, signing_key_path, compression, attic-specific fields, S3-specific fields, enabled status
- [x] #3 Cache destinations API endpoints exist: GET /api/caches (list), POST /api/caches (create), PUT /api/caches/:id (update), DELETE /api/caches/:id (delete), GET /api/caches/:id (view)
- [ ] #4 Cache view displays a list of configured cache destinations with type, URL, enabled status, and last used timestamp
- [ ] #5 Add cache destination form validates required fields based on cache type (e.g., attic_cache_name for Attic type)
- [ ] #6 Edit cache destination form pre-populates existing values and saves changes
- [ ] #7 Delete cache destination shows confirmation dialog and removes the entry
- [ ] #8 Cache push jobs section displays paginated job list with columns: derivation name, status, cache destination, attempts, scheduled/started/completed timestamps
- [ ] #9 Cache push jobs can be filtered by status (pending, in_progress, failed, completed, permanently_failed)
- [ ] #10 Failed cache push jobs show error messages and allow manual retry via 'Retry' button
- [ ] #11 Pending jobs can be cancelled via 'Cancel' button
- [ ] #12 Bulk selection UI allows selecting multiple jobs and performing retry or cancel operations
- [ ] #13 Job detail view shows full error message, push size, duration, store path, and attempt history
- [ ] #14 Cache worker uses database-backed cache destinations instead of server.toml (or hybrid fallback)
- [ ] #15 All cache operations require admin role authorization
- [ ] #16 UI follows existing Crystal Forge design patterns and component structure
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
## Implementation Plan

### Phase 1: Database Schema & Migrations
1. Create migration for `cache_destinations` table with columns:
   - id (serial primary key)
   - name (text, unique, not null)
   - cache_type (text, not null) - 'S3', 'Attic', 'Http', 'Nix'
   - push_to (text) - destination URL
   - enabled (boolean, default true)
   - signing_key_path (text)
   - compression (text)
   - s3_region, s3_profile (text)
   - attic_token, attic_cache_name (text)
   - attic_ignore_upstream_cache_filter (boolean)
   - attic_jobs (integer)
   - parallel_uploads (integer)
   - max_retries (integer)
   - retry_delay_seconds (bigint)
   - push_timeout_seconds (bigint)
   - force_repush (boolean)
   - require_sigs (boolean)
   - created_at, updated_at, last_used_at (timestamptz)
2. Add foreign key to cache_push_jobs.cache_destination referencing cache_destinations.name

### Phase 2: API Models & Queries
1. Create `packages/default/src/models/cache_destination.rs` with CacheDestination struct
2. Create `packages/default/src/queries/cache_destinations.rs` with CRUD operations
3. Update existing cache_push queries to join with cache_destinations table

### Phase 3: API Handlers
1. Create `packages/default/src/handlers/api/caches.rs` with:
   - list_cache_destinations (GET /api/caches)
   - get_cache_destination (GET /api/caches/:id)
   - create_cache_destination (POST /api/caches) - admin only
   - update_cache_destination (PUT /api/caches/:id) - admin only
   - delete_cache_destination (DELETE /api/caches/:id) - admin only
   - list_cache_push_jobs (GET /api/cache-push-jobs?status=&cache_id=)
   - retry_cache_push_job (POST /api/cache-push-jobs/:id/retry) - admin only
   - cancel_cache_push_job (POST /api/cache-push-jobs/:id/cancel) - admin only
   - bulk_retry_jobs (POST /api/cache-push-jobs/bulk-retry) - admin only
   - bulk_cancel_jobs (POST /api/cache-push-jobs/bulk-cancel) - admin only
2. Register routes in handlers/api/mod.rs

### Phase 4: Web UI Components
1. Create `packages/web-ui/src/views/caches.rs` with:
   - CacheList component (table of cache destinations)
   - CacheForm component (add/edit cache destination)
   - CachePushJobsList component (table of push jobs)
   - CachePushJobDetail component (modal or detail view)
2. Create `packages/web-ui/src/components/cache_*.rs` for reusable sub-components
3. Add route to navigation in App.rs
4. Add API client functions in api module

### Phase 5: Cache Worker Integration
1. Update `packages/default/src/builder/cache_worker.rs` to:
   - Query cache_destinations table on startup
   - Use database config with server.toml fallback
   - Update last_used_at timestamp when pushing to a destination
2. Add config migration helper to seed initial cache_destinations from server.toml

### Phase 6: Testing & Verification
1. Write unit tests for cache destination CRUD
2. Write integration tests for cache push job endpoints
3. Manual UI testing for all CRUD operations
4. Test cache worker reads from database correctly
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
## Architectural Constraints

1. **Database-First Design**: Cache destinations are stored in PostgreSQL, not TOML config. The server.toml cache config becomes a fallback/seed for initial setup.

2. **Backward Compatibility**: Cache worker must gracefully fall back to server.toml if cache_destinations table is empty or migration hasn't run.

3. **Authorization**: All write operations (create/update/delete cache destinations, retry/cancel jobs) require admin role. Read operations may be accessible to authenticated users.

4. **UI Layer Separation**: 
   - Views must not contain business logic
   - API calls through dedicated client module
   - Form validation in both frontend (UX) and backend (security)

5. **Error Handling**:
   - No unwrap() in production paths
   - User-facing error messages must be actionable
   - API returns structured error responses

6. **Component Reusability**:
   - Extract form fields into reusable components (CacheTypeSelector, S3ConfigFields, AtticConfigFields)
   - Follow existing patterns from systems_list.rs, flakes_list.rs

## Technical Decisions

- **Multiple Cache Destinations**: The database schema supports multiple named caches. Future work can add per-flake or per-derivation cache selection.
- **Migration Strategy**: Provide a CLI command or admin UI button to seed cache_destinations from server.toml for initial setup.
- **Job Cancellation**: Cancelled jobs are marked as 'cancelled' status and excluded from retry logic.
- **Bulk Operations**: Use JSON array of job IDs in request body for bulk retry/cancel.

## Risk Areas

- **Cache Worker Compatibility**: Ensure cache_worker.rs doesn't break if database config is missing
- **Migration Rollback**: Plan for rolling back cache_destinations migration if needed
- **Secrets Management**: Signing keys and tokens stored in database - consider encryption at rest (future enhancement)
- **Server Restart**: Changes to cache config may require cache worker restart (document this in UI)

## Verification Plan

**Tier 1: Feature-Level Integration** (Required for this task)

This task requires runtime verification of UI + API + database integration.

### Phase 1: Database & API Verification
```bash
# Format and lint
cargo fmt -- --check
cargo clippy --all-targets --all-features -- -D warnings

# Unit tests (targeted)
cargo test --package crystal-forge models::cache_destination
cargo test --package crystal-forge queries::cache_destinations
cargo test --package crystal-forge handlers::api::caches

# SQLx metadata sync (REQUIRED if schema changes)
db-only up
cargo sqlx prepare
```

### Phase 2: UI Integration Testing
```bash
# Start full stack with database
full-stack up

# Manual verification:
# 1. Navigate to /caches route
# 2. Verify cache destinations table renders
# 3. Add a new Attic cache destination with all fields
# 4. Edit the cache destination
# 5. View cache push jobs list
# 6. Filter jobs by status
# 7. Retry a failed job (if any exist, or create test data)
# 8. Test bulk selection and bulk retry
# 9. Delete the test cache destination
# 10. Verify admin-only authorization (test with non-admin user)
```

### Phase 3: Cache Worker Integration
```bash
# Verify cache worker reads from database
# 1. Add cache destination via UI
# 2. Trigger a build that creates cache push jobs
# 3. Verify cache worker picks up jobs and uses correct destination
# 4. Check last_used_at timestamp updates
```

**Why not Tier 2 (nix flake check)?**
This is a UI/API feature addition that doesn't modify Nix packaging, build infrastructure, or cross-package interfaces. Tier 1 verification with targeted tests and full-stack integration testing is sufficient.

## Dependencies

- None (all dependencies are already available)

## Impact Areas

- Database schema (new table + migration)
- API handlers (new routes)
- Web UI (new view)
- Cache worker (modified to read from database)
- Navigation (new menu item)

## Files Expected to Change

**Database:**
- packages/default/migrations/XXXX_create_cache_destinations.sql
- packages/default/migrations/XXXX_add_cache_destination_fk_to_push_jobs.sql

**Backend:**
- packages/default/src/models/cache_destination.rs (new)
- packages/default/src/models/mod.rs (add cache_destination)
- packages/default/src/queries/cache_destinations.rs (new)
- packages/default/src/queries/mod.rs (add cache_destinations)
- packages/default/src/queries/cache_push.rs (update to join cache_destinations)
- packages/default/src/handlers/api/caches.rs (new)
- packages/default/src/handlers/api/mod.rs (register caches routes)
- packages/default/src/builder/cache_worker.rs (read from db)

**Frontend:**
- packages/web-ui/src/views/caches.rs (new)
- packages/web-ui/src/views/mod.rs (add caches)
- packages/web-ui/src/components/cache_*.rs (new, as needed)
- packages/web-ui/src/App.rs (add route)
- packages/web-ui/src/api/caches.rs (new API client)
- packages/web-ui/src/api/mod.rs (add caches module)

**Tests:**
- packages/default/tests/integration/cache_destinations_test.rs (new)

LOCK: Claude (agent) on gray in /home/mcamp/code/crystal-forge/TASK-42-cache-view

## Progress Update - Backend Complete

✅ Phase 1: Database migrations created (0091, 0092)
✅ Phase 2: Models and queries complete with full CRUD + retry/cancel/bulk operations
✅ Phase 3: API handlers complete with proper error handling
✅ All 27 ApiError formatting issues resolved
✅ Code compiles successfully

⚠️ Next: SQLx metadata sync (cargo sqlx prepare) once database is running

Then: Phase 4 (UI), Phase 5 (cache worker integration), Phase 6 (testing)

## Phase 4 Complete - Web UI Created

✅ Cache destinations list view with cards
✅ Cache push jobs list view with table
✅ Tab navigation between destinations and jobs
✅ API client functions for all CRUD operations
✅ Status filtering for push jobs
✅ Retry and Cancel action buttons
✅ Route registered at /caches

⚠️ Remaining for full UI:
- Add/Edit modal dialogs for cache destinations
- Bulk selection checkboxes for jobs
- Job detail modal
- Form validation and error handling

Next: Phase 5 (cache worker integration)
<!-- SECTION:NOTES:END -->

## Definition of Done
<!-- DOD:BEGIN -->
- [x] #1 Database migration created and tested for cache_destinations table
- [x] #2 API handlers implemented with proper error handling and validation
- [x] #3 Dioxus view component created following existing view patterns
- [ ] #4 Cache worker updated to read from database (with server.toml fallback)
- [ ] #5 Unit tests for cache CRUD operations
- [ ] #6 Integration tests for cache push job monitoring endpoints
- [ ] #7 UI components follow accessibility standards (ARIA labels, keyboard navigation)
- [ ] #8 Cargo fmt and clippy pass with no warnings
- [ ] #9 No unwrap() in production code paths
- [ ] #10 Error messages are user-friendly and actionable
<!-- DOD:END -->

---
id: TASK-42
title: Create cache view
status: Backlog
assignee:
  - KimiK2.5
created_date: '2026-02-17 04:43'
updated_date: '2026-03-08 14:59'
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
- [ ] #1 A new '/caches' route exists and is accessible from the main navigation
- [ ] #2 Cache destinations table schema exists with migrations for: name, cache_type (S3/Attic/Http/Nix), URL, signing_key_path, compression, attic-specific fields, S3-specific fields, enabled status
- [ ] #3 Cache destinations API endpoints exist: GET /api/caches (list), POST /api/caches (create), PUT /api/caches/:id (update), DELETE /api/caches/:id (delete), GET /api/caches/:id (view)
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

## Definition of Done
<!-- DOD:BEGIN -->
- [ ] #1 Database migration created and tested for cache_destinations table
- [ ] #2 API handlers implemented with proper error handling and validation
- [ ] #3 Dioxus view component created following existing view patterns
- [ ] #4 Cache worker updated to read from database (with server.toml fallback)
- [ ] #5 Unit tests for cache CRUD operations
- [ ] #6 Integration tests for cache push job monitoring endpoints
- [ ] #7 UI components follow accessibility standards (ARIA labels, keyboard navigation)
- [ ] #8 Cargo fmt and clippy pass with no warnings
- [ ] #9 No unwrap() in production code paths
- [ ] #10 Error messages are user-friendly and actionable
<!-- DOD:END -->

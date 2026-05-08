---
id: TASK-291
title: Fix builder concurrency limit bug and ensure full UI configurability
status: In Progress
assignee: []
created_date: '2026-05-08 02:46'
updated_date: '2026-05-08 02:55'
labels:
  - bug
  - builders
  - ui
  - configuration
  - high-priority
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

The builder is using the wrong configuration value for its concurrency semaphore, causing builders to potentially stall when `build.max_concurrent_derivations` is set to 0 or an incorrect value.

**Bug Location**: `packages/default/src/bin/builder.rs:225`

```rust
// WRONG - uses Nix build parallelism instead of builder job limit
let max_concurrent = build_config.max_concurrent_derivations;

// SHOULD BE (like line 159)
let max_concurrent = builder_config.max_concurrent_jobs.unwrap_or(1);
```

**Root Cause**:
- `build_config.max_concurrent_derivations` controls Nix-level parallelism *within* a single build
- `builder_config.max_concurrent_jobs` controls how many separate build jobs the builder handles concurrently
- Line 225 incorrectly uses the former instead of the latter

**Impact**:
- If `max_concurrent_derivations = 0`: Builder semaphore has 0 permits → builder never claims jobs from queue (stalled)
- If `max_concurrent_derivations` is very high: Builder may claim too many jobs, exceeding intended concurrency

**Evidence**:
User reported: "builder shows as online in the ui.. the journal says it has two workers and both are idle"
This matches the symptom of semaphore.available_permits() == 0 at line 236.

---

## Goal

1. **Fix the immediate bug**: Use `builder_config.max_concurrent_jobs` instead of `build_config.max_concurrent_derivations` at line 225
2. **Ensure full UI configurability**: All builder configuration should be manageable via UI, with config file only used for bootstrap
3. **UI/config parity**: The UI should be able to override any config file setting after bootstrap

---

## Scope

### In Scope
1. Fix line 225 to use correct config value
2. Audit all builder configuration fields to ensure they're exposed in the UI
3. Verify builders can be fully configured via UI (create, update all fields)
4. Ensure `max_concurrent_jobs` is editable in the builder management UI
5. Add any missing builder configuration fields to the UI
6. Verify API endpoints support updating all builder configuration fields

### Out of Scope
- Changing the semantics of `max_concurrent_derivations` (Nix build config)
- Changing database schema (should already support all fields)
- Build configuration UI (separate from builder configuration)

---

## Current State Analysis

**Builder Configuration Fields** (from `models/builders.rs`):
- `name` - builder name
- `public_key` - Ed25519 public key
- `status` - active/inactive/offline (auto-managed + manual override)
- `max_cpu_cores` - optional CPU limit
- `max_memory_mb` - optional memory limit
- `max_concurrent_jobs` - **THIS IS THE KEY FIELD** (defaults to 1)
- `last_heartbeat_at` - auto-managed
- Environment assignments - which environments this builder can serve

**API Endpoints** (from `handlers/api/builders.rs`):
- `POST /api/v1/builders` - create builder
- `GET /api/v1/builders` - list builders
- `GET /api/v1/builders/:id` - get builder details
- `PATCH /api/v1/builders/:id` - update builder
- `DELETE /api/v1/builders/:id` - delete builder
- `POST /api/v1/builders/:id/environments/:env_id` - assign environment
- `DELETE /api/v1/builders/:id/environments/:env_id` - unassign environment

**UpdateBuilderRequest** supports:
- `name`
- `status`
- `max_cpu_cores`
- `max_memory_mb`
- `max_concurrent_jobs` ✅

So the API already supports updating all fields.

---

## Implementation Plan

### Phase 1: Fix the Bug (Critical)
1. Change line 225 in `packages/default/src/bin/builder.rs`:
   ```rust
   let max_concurrent = builder_config.max_concurrent_jobs.unwrap_or(1);
   ```
2. Add a comment explaining the difference between the two config values
3. Add a unit test or integration test verifying the semaphore respects `builder_config.max_concurrent_jobs`

### Phase 2: UI Audit & Enhancement
1. **Locate builder management UI** (likely in web-ui)
2. **Verify all fields are exposed**:
   - ✅ Name
   - ✅ Status (active/inactive)
   - ❓ max_concurrent_jobs
   - ❓ max_cpu_cores
   - ❓ max_memory_mb
   - ✅ Environment assignments
   - ❓ Public key rotation
3. **Add missing fields to UI** if not present
4. **Ensure edit modal/form includes all updatable fields**
5. **Add validation** (e.g., max_concurrent_jobs must be >= 1)

### Phase 3: Testing
1. Test builder creation via UI
2. Test updating max_concurrent_jobs via UI
3. Verify builder respects updated max_concurrent_jobs without restart
4. Test that config file values are overridden by database values

---

## Configuration Priority (Design Decision)

**Intended behavior**:
1. **Bootstrap**: Config file provides initial builder settings when builder first registers
2. **Runtime**: Database is source of truth; UI updates persist to database
3. **Override**: UI changes always take precedence over config file
4. **Live reload**: Builder should respect database changes (via heartbeat or config refresh)

**Current behavior to verify**:
- Does the builder re-read its config from the database on each heartbeat?
- Or does it only read config once at startup?

If config is only read at startup, we may need to add a config refresh mechanism.

---

## Files to Modify

### Critical Fix
- `packages/default/src/bin/builder.rs` (line 225)

### UI Enhancement (TBD - need to locate builder management UI)
- `packages/web-ui/src/views/builders.rs` (likely)
- `packages/web-ui/src/components/builders/*` (if exists)

### Testing
- Add test case to verify semaphore uses correct config value

---

## Acceptance Criteria
<!-- AC:BEGIN -->
### Bug Fix
- [x] #1 Line 225 in builder.rs uses builder_config.max_concurrent_jobs.unwrap_or(1) instead of build_config.max_concurrent_derivations
- [x] #2 Comment added explaining the distinction between builder_config.max_concurrent_jobs (builder-level concurrency) and build_config.max_concurrent_derivations (Nix-level parallelism)
- [ ] #3 Builder with max_concurrent_jobs = 2 successfully claims and processes up to 2 jobs concurrently
- [ ] #4 Builder with max_concurrent_jobs = 1 successfully claims and processes only 1 job at a time
- [ ] #5 Builder respects the max_concurrent_jobs value from the database (not config file) if they differ

### UI Configurability
- [x] #6 UI builder management view exposes all configurable builder fields: name, status, max_concurrent_jobs, max_cpu_cores, max_memory_mb, environment assignments
- [x] #7 UI validates that max_concurrent_jobs must be >= 1
- [ ] #8 Changes to max_concurrent_jobs made via UI are persisted to database and reflected in builder behavior
- [ ] #9 Builder either immediately respects updated max_concurrent_jobs or picks up changes on next heartbeat cycle

### Documentation

---

## Risk Assessment

**Risk Level**: High (user-reported production issue)

**Risks**:
- **Builder stall**: Current bug causes builders to stall in production
- **Scope creep**: UI configurability audit may reveal many missing fields
- **Breaking change**: If we change how config priority works, existing deployments may break

**Mitigation**:
- Fix the critical bug first (phase 1)
- UI enhancement can be iterative (phase 2)
- Test thoroughly with different config scenarios
<!-- SECTION:DESCRIPTION:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
## Phase 1: Critical Bug Fix - COMPLETE

- Fixed line 225 to use builder_config.max_concurrent_jobs
- Added max_concurrent_jobs parameter to run_api_job_loop
- Added explanatory comment distinguishing builder vs build config
- All code formatted with cargo fmt

## Phase 2: UI Audit - COMPLETE

Reviewed:
- edit_builder_modal.rs (lines 300-313)
- add_builder_modal.rs  
- builder_card.rs

UI Configuration Status:
✅ Name - exposed and editable
✅ Status (active/inactive/offline) - exposed and editable  
✅ max_concurrent_jobs - exposed and editable (with min=1 validation)
✅ max_cpu_cores - exposed and editable
✅ max_memory_mb - exposed and editable
✅ Environment assignments - exposed and editable
✅ Public key rotation - exposed with generate keypair functionality

All builder configuration fields are fully exposed in UI.
API already supports all update operations via UpdateBuilderRequest.

## Phase 3: Testing - IN PROGRESS

Requires database running for full verification.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
LOCK: agent on gray in ~/code/crystal-forge/TASK-291-fix-builder-concurrency
<!-- SECTION:NOTES:END -->

<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
LOCK: agent on gray in ~/code/crystal-forge/TASK-291-fix-builder-concurrency
<!-- SECTION:NOTES:END -->

<!-- AC:END -->

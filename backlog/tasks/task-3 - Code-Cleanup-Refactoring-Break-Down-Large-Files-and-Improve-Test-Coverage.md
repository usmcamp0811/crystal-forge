---
id: TASK-3
title: Code Cleanup & Refactoring - Break Down Large Files and Improve Test Coverage
status: To Do
assignee:
  - KimiK2.5
created_date: '2026-02-04 20:15'
updated_date: '2026-03-01 14:27'
labels:
  - refactoring
  - architecture
  - phase-2
milestone: m-2
dependencies:
  - TASK-2
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

The codebase has accumulated several **very large files** that violate Single Responsibility Principle and are difficult to test, maintain, and reason about:

### Backend (packages/default/src)
- **queries/derivations.rs** - 2,012 lines (query module)
- **builder/mod.rs** - 1,614 lines (orchestration + workers + cache + CVE)
- **handlers/api/admin.rs** - 1,363 lines (admin API endpoints)
- **queries/builders.rs** - 1,072 lines (query module)
- **flake/commits.rs** - 946 lines (git operations)
- **handlers/api/flakes.rs** - 939 lines (flake API endpoints)
- **handlers/api/auth_oidc.rs** - 909 lines (OIDC auth flow)
- **handlers/api/systems.rs** - 835 lines (systems API endpoints)
- **api/models.rs** - 829 lines (all API DTOs)

### Frontend (packages/web-ui/src)
- **views/flakes_list.rs** - 2,953 lines (flakes view)
- **views/system_detail.rs** - 2,002 lines (system detail view)
- **views/admin.rs** - 1,396 lines (admin console)
- **api/models.rs** - 1,073 lines (all API DTOs)

These files make it difficult to:
- **Navigate code** - Finding specific logic is time-consuming
- **Write tests** - Large files are harder to test in isolation
- **Review changes** - MRs touching these files are massive
- **Prevent bugs** - Complexity breeds subtle issues
- **Onboard developers** - New contributors get overwhelmed

## Goal

Break down large files (>800 lines) into focused, single-responsibility modules while improving test coverage.

**Target:**
- ✅ No file >500 lines (ideally <300)
- ✅ Each module has clear, single responsibility
- ✅ Test coverage >70% for refactored modules
- ✅ All existing tests continue passing

## Architectural Constraints

- **Backend modules follow query/handler/service pattern** (established in TASK-127)
- **Frontend follows component extraction pattern** (views compose components)
- **No behavior changes** - Pure refactoring, same functionality
- **Preserve existing APIs** - No breaking changes to public interfaces

## Scope - Priority Order

### Phase 1: Backend Queries (Highest Impact)

**queries/derivations.rs (2,012 lines)**
- Extract to:
  - `queries/derivations/build_queue.rs` - Build queue queries
  - `queries/derivations/status.rs` - Status transitions
  - `queries/derivations/cache.rs` - Cache-related queries
  - `queries/derivations/metadata.rs` - Metadata queries
  - `queries/derivations/mod.rs` - Re-exports

**queries/builders.rs (1,072 lines)**
- Extract to:
  - `queries/builders/jobs.rs` - Job assignment queries
  - `queries/builders/metrics.rs` - Metrics queries
  - `queries/builders/management.rs` - CRUD operations
  - `queries/builders/mod.rs` - Re-exports

### Phase 2: Backend Builder Module

**builder/mod.rs (1,614 lines)** - Already has TASK-3 subtasks
- Extract workers to separate files (status, cache, CVE, build)
- Reduce mod.rs to orchestration only

### Phase 3: Backend Handlers

**handlers/api/admin.rs (1,363 lines)**
- Extract to:
  - `handlers/api/admin/users.rs` - User management
  - `handlers/api/admin/audit.rs` - Audit log
  - `handlers/api/admin/oidc.rs` - OIDC mappings
  - `handlers/api/admin/mod.rs` - Re-exports

**handlers/api/flakes.rs (939 lines)**
- Extract to:
  - `handlers/api/flakes/registry.rs` - Registry CRUD
  - `handlers/api/flakes/sync.rs` - Sync operations
  - `handlers/api/flakes/timeline.rs` - Timeline/commits
  - `handlers/api/flakes/mod.rs` - Re-exports

**handlers/api/systems.rs (835 lines)**
- Extract to:
  - `handlers/api/systems/list.rs` - List/filter operations
  - `handlers/api/systems/detail.rs` - Detail/CRUD
  - `handlers/api/systems/actions.rs` - Sync/rollback actions
  - `handlers/api/systems/mod.rs` - Re-exports

### Phase 4: Frontend Views

**views/flakes_list.rs (2,953 lines)**
- Extract components:
  - `components/flake/flake_list_item.rs` - List item
  - `components/flake/flake_filters.rs` - Filter controls
  - `components/flake/add_flake_modal.rs` - Add modal
  - `views/flakes_list.rs` - Thin orchestration

**views/system_detail.rs (2,002 lines)**
- Extract components:
  - `components/system/overview_tab.rs` - Overview
  - `components/system/deploy_tab.rs` - Deployment
  - `components/system/logs_tab.rs` - Logs
  - `components/system/actions_panel.rs` - Actions
  - `views/system_detail.rs` - Thin orchestration

**views/admin.rs (1,396 lines)**
- Extract components:
  - `components/admin/users_tab.rs` - Users management
  - `components/admin/audit_tab.rs` - Audit log
  - `components/admin/oidc_tab.rs` - OIDC mappings
  - `views/admin.rs` - Thin orchestration

### Phase 5: API Models

**api/models.rs (backend 829, frontend 1,073 lines)**
- Group by domain:
  - `api/models/systems.rs` - System DTOs
  - `api/models/builders.rs` - Builder DTOs
  - `api/models/flakes.rs` - Flake DTOs
  - `api/models/auth.rs` - Auth DTOs
  - `api/models/admin.rs` - Admin DTOs
  - `api/models/mod.rs` - Re-exports

## Implementation Strategy

### For Each Large File

1. **Analyze** - Identify logical groupings/responsibilities
2. **Extract** - Create new module files with focused logic
3. **Test** - Add unit tests for extracted modules (>70% coverage)
4. **Verify** - Ensure existing tests pass
5. **Document** - Update module docs with clear responsibilities

### Refactoring Pattern

```rust
// Before: queries/derivations.rs (2,012 lines)
pub async fn get_build_queue(...) { }
pub async fn update_status(...) { }
pub async fn get_cache_push_jobs(...) { }
// ... 2,000 more lines

// After: queries/derivations/mod.rs (50 lines)
mod build_queue;
mod status;
mod cache;
pub use build_queue::*;
pub use status::*;
pub use cache::*;

// queries/derivations/build_queue.rs (300 lines)
pub async fn get_build_queue(...) { }
// Focused, testable, documented
```

## Non-Goals

- ❌ Changing behavior or fixing bugs (pure refactoring)
- ❌ Rewriting in different patterns (keep existing patterns)
- ❌ Adding new features during refactoring
- ❌ Touching files <500 lines (focus on biggest offenders)
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Extract worker status management to status.rs
- [ ] #2 Extract reservation cleanup to reservation.rs
- [ ] #3 Extract CVE scan worker to cve_worker.rs
- [ ] #4 Extract cache push worker to cache_worker.rs
- [ ] #5 Extract build worker to worker.rs
- [ ] #6 Create builder error types in error.rs
- [ ] #7 Refactor mod.rs to orchestrate only
- [ ] #8 queries/derivations.rs broken into <500 line modules (build_queue, status, cache, metadata)
- [ ] #9 queries/builders.rs broken into <500 line modules (jobs, metrics, management)
- [ ] #10 builder/mod.rs reduced to <300 lines (orchestration only)
- [ ] #11 handlers/api/admin.rs broken into <500 line modules (users, audit, oidc)
- [ ] #12 handlers/api/flakes.rs broken into <500 line modules (registry, sync, timeline)
- [ ] #13 handlers/api/systems.rs broken into <500 line modules (list, detail, actions)
- [ ] #14 views/flakes_list.rs broken into components (<500 lines main view)
- [ ] #15 views/system_detail.rs broken into components (<500 lines main view)
- [ ] #16 views/admin.rs broken into components (<500 lines main view)
- [ ] #17 Backend api/models.rs split by domain (<300 lines per module)
- [ ] #18 Frontend api/models.rs split by domain (<300 lines per module)
- [ ] #19 All extracted modules have unit tests (>70% coverage target)
- [ ] #20 All existing tests continue passing
- [ ] #21 cargo fmt and cargo clippy pass
- [ ] #22 Documentation updated for new module structure
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Target: No file >300 lines, each module >80% coverage, all tests pass
<!-- SECTION:NOTES:END -->

## Definition of Done
<!-- DOD:BEGIN -->
- [ ] #1 No Rust file >800 lines in packages/default/src
- [ ] #2 No Rust file >800 lines in packages/web-ui/src
- [ ] #3 Test coverage baseline measured and documented
- [ ] #4 MR includes before/after line count comparison
<!-- DOD:END -->

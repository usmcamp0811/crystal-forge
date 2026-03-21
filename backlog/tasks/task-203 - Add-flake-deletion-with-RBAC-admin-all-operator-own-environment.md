---
id: TASK-203
title: Add flake deletion with RBAC - admin all, operator own environment
status: In Progress
assignee:
  - Claude
created_date: '2026-03-20 13:40'
updated_date: '2026-03-20'
labels:
  - backend
  - frontend
  - rbac
  - database
  - high-priority
dependencies: []
references:
  - packages/default/src/handlers/api/flakes.rs
  - packages/default/src/queries/flakes.rs
  - packages/web-ui/src/views/flakes.rs
priority: high
ordinal: 1400
---

# Add flake deletion with RBAC: admin all, operator own environment

---

# Problem Statement

There is no way to delete flakes in the UI or API. Flakes accumulate indefinitely without a cleanup mechanism. This leads to clutter, confusion, and potential storage issues.

---

# Goal

Implement flake deletion capability with proper RBAC enforcement:
- **Admins**: Can delete any flake
- **Operators**: Can delete flakes only in environments they have access to
- **Viewers**: Cannot delete flakes

Support both soft delete (default, marks deleted but retains in DB) and hard delete (permanent removal, with warning and extra confirmation).

For flakes with active dependencies (evaluations, builds, deployments), block deletion by default but allow cascade delete with extra confirmation.

---

# Non-Goals

- Implementing flake archival/export before deletion
- Adding bulk delete functionality
- Adding scheduled/automated deletion
- Implementing trash/recycle bin with restore
- Changing flake data model significantly

---

# Acceptance Criteria

- [ ] Backend API endpoint: `DELETE /api/flakes/:flake_id`
- [ ] RBAC authorization:
  - Admin: allow deletion of any flake
  - Operator: allow deletion only if user has access to flake's environment(s)
  - Viewer: deny (403 Forbidden)
- [ ] Soft delete (default):
  - Add `deleted_at` timestamp column to `flakes` table (migration)
  - Deleted flakes excluded from normal queries
  - Deleted flakes retained in database for audit
- [ ] Hard delete option:
  - Query parameter: `DELETE /api/flakes/:flake_id?hard=true`
  - Permanently removes flake from database
  - Requires extra confirmation in UI (two-step: soft delete warning, then hard delete warning)
- [ ] Dependency checking:
  - Block deletion if flake has active (pending/in-progress) evaluations, builds, or deployments
  - Return 409 Conflict with list of blocking dependencies
  - Option to cascade delete: `DELETE /api/flakes/:flake_id?cascade=true`
  - Cascade delete requires extra confirmation in UI
  - Cascade delete removes all related evaluations, builds, deployments
- [ ] Frontend UI:
  - Delete button in Flakes view (per-flake action)
  - Delete button only visible based on role:
    - Admin: always visible
    - Operator: visible only for flakes in their environments
    - Viewer: not visible
  - Soft delete confirmation modal:
    - "Are you sure you want to delete [flake-name]?"
    - Checkbox: "Permanently delete (cannot be undone)"
    - If dependencies exist: show warning + checkbox "Also delete all evaluations, builds, and deployments"
  - Hard delete confirmation (if checkbox selected):
    - "⚠️ This will PERMANENTLY delete [flake-name] and cannot be undone. Type 'DELETE' to confirm."
    - Text input validation
  - Success toast: "Flake deleted successfully"
  - Error toast: show specific error (permission denied, dependencies exist, etc.)
- [ ] Audit logging:
  - Log who deleted flake, when, and deletion type (soft/hard/cascade)
  - Include flake ID and name in log
- [ ] Queries updated to exclude soft-deleted flakes:
  - `list_flakes` adds `WHERE deleted_at IS NULL`
  - Add admin query `list_deleted_flakes` for audit
- [ ] Documentation updated with deletion behavior

---

# Architectural Constraints

- Database changes require migration (add `deleted_at` column)
- Cascade delete must be transactional (all or nothing)
- RBAC check MUST happen before deletion logic
- Soft delete is default, hard delete is opt-in
- Audit log must record all deletions
- No business logic in UI (deletion logic in backend)
- Use existing environment membership queries for Operator authorization
- Follow existing API error response patterns

---

# Verification Plan

Automated:
- `nix develop -c cargo test flakes::delete`
- `nix develop -c cargo test auth::rbac`
- `nix develop -c cargo clippy -- -D warnings`
- `nix develop -c cargo fmt -- --check`
- `sqlx prepare` (update offline metadata)

Manual:
- Test as Admin:
  - Delete flake with no dependencies (soft delete)
  - Verify flake no longer shown in flakes list
  - Verify flake still in DB with deleted_at set
  - Delete flake with hard delete option
  - Verify flake permanently removed from DB
  - Delete flake with active evaluations/builds
  - Verify blocked with error message
  - Delete with cascade option
  - Verify flake and all dependencies deleted
- Test as Operator:
  - Attempt to delete flake in operator's environment
  - Verify succeeds (soft delete)
  - Attempt to delete flake in different environment
  - Verify denied (403)
- Test as Viewer:
  - Verify no delete button/option visible
  - Attempt API call directly: `DELETE /api/flakes/:id`
  - Verify denied (403)
- Test dependency checking:
  - Create flake with pending build
  - Attempt delete without cascade
  - Verify blocked with clear error
  - Delete with cascade
  - Verify build also deleted
- Test audit log:
  - Delete flake
  - Check audit_log table for deletion entry
  - Verify correct user, timestamp, and action

---

# Impact Areas

UI | API | Domain | Infrastructure | Database

- Flakes API endpoint (new DELETE)
- RBAC authorization logic
- Flakes queries module (add deleted_at filter)
- Database schema (migration for deleted_at)
- Flakes UI component (delete button, confirmation modal)
- Audit logging
- Dependency cascade logic

---

# Risk Level

Medium

Deletion is a destructive operation and must be carefully implemented. Risks include:
- Accidentally hard deleting when soft delete intended
- Cascade delete removing more data than expected
- RBAC check bypassed or incorrect
- Dependency checking missing a relationship

Mitigations:
- Soft delete is default (safe)
- Hard delete requires explicit opt-in and confirmation
- Cascade delete requires explicit flag and extra confirmation
- Comprehensive RBAC tests for all roles
- Transaction for cascade delete (rollback on error)
- Audit log for accountability
- Clear UI warnings and confirmations

---

# Dependencies

None

---

# Follow-Up Tasks

- Add admin UI to view and restore soft-deleted flakes
- Add bulk delete for multiple flakes
- Add scheduled deletion of old soft-deleted flakes (retention policy)
- Add export/backup before deletion
- Add flake archival (different from deletion)

---

# Implementation Notes

LOCK: Claude on reckless in /home/mcamp/code/crystal-forge/TASK-203-flake-deletion-rbac

Task moved to In Progress. Creating dedicated worktree.

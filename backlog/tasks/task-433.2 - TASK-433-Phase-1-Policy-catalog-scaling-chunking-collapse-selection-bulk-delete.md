---
id: TASK-433.2
title: >-
  TASK-433 Phase 1: Policy catalog scaling (chunking, collapse, selection, bulk
  delete)
status: Review
assignee:
  - claude-agent
created_date: '2026-08-23 01:42'
updated_date: '2026-08-23 04:34'
labels:
  - design-parity
  - policy
  - web-ui
  - server
  - phase-1
dependencies:
  - TASK-433.1
references:
  - TASK-433
  - TASK-433.1
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/318'
documentation:
  - docs/design/CrystalForge/components/PoliciesView.jsx
  - docs/design/CrystalForge/data-policies.js
modified_files:
  - checks/web-ui/tests/integration-test.js
  - packages/default/crates/cf-server/src/api/models.rs
  - packages/default/crates/cf-server/src/bin/server.rs
  - packages/default/crates/cf-server/src/handlers/api/deployment_policies.rs
  - packages/default/crates/cf-server/src/queries/deployment_policies.rs
  - packages/web-ui/assets/app.css
  - packages/web-ui/src/api/client.rs
  - packages/web-ui/src/api/models.rs
  - packages/web-ui/src/components/policy/mod.rs
  - packages/web-ui/src/components/policy/policy_card.rs
  - packages/web-ui/src/components/policy/policy_row.rs
  - packages/web-ui/src/views/policies.rs
parent_task_id: TASK-433
priority: high
type: feature
ordinal: 434000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Objective
Parent umbrella: TASK-433, phase 1 of 8 (contextual only, not an execution-blocking dependency framework beyond the explicit dependency below). Implements the policy catalog scaling behavior from `PoliciesView.jsx`/`data-policies.js` in the design delta, preserving existing catalog pagination and deletion-eligibility server behavior.

Implement policy group collapse/expand, chunked rendering of large groups, search-aware collapse restoration, cards/table view parity, logical multi-select (individual/Shift-range/group/cross-chunk/clear), selected export, and server-reasoned bulk delete with partial/all-blocked/failure reporting.

## Explicit scope
- Groups independently collapse; groups larger than 150 policies default collapsed; visible group counts and selection state.
- Large groups initially render at most 60 items with current/total plus Show more / Show all.
- Search reveals matches inside collapsed groups; clearing search restores prior explicit collapse state.
- Cards and table views preserve equivalent policy semantics and logical selection state.
- Individual, Shift-range, group, cross-chunk, clear, selected export, and selected delete all operate on filtered logical order (not just rendered DOM order).
- Bulk delete uses server-side eligibility, reports deleted/skipped/reasons, handles partial success/all-blocked/failure, and preserves immutable-history blockers (do not weaken deletion/immutable-history rules).
- Preserve existing catalog API pagination; chunking/collapse is client-side rendering only.

## Explicit non-scope
No changes to policy editor, enforcement execution, Nix metadata, or POA&M. No fixtures/seeded data. Do not weaken deletion/immutable-history semantics.

## Verification
```bash
nix develop -c cargo fmt --manifest-path packages/web-ui/Cargo.toml -- --check
nix develop -c cargo test --manifest-path packages/web-ui/Cargo.toml
nix build .#packages.x86_64-linux.web-ui --no-link
nix develop -c cargo fmt --manifest-path packages/default/Cargo.toml -- --check
nix develop -c cargo test --manifest-path packages/default/Cargo.toml
nix build .#checks.x86_64-linux.web-ui --no-link
```
Add/extend a browser workflow proving deep search, collapse/expand, cards/table, and range selection with more than 60 policies (contributes to parent TASK-433 AC #32, finalized in TASK-433.9).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Policy groups independently collapse; groups larger than 150 default collapsed; group counts and selection state are visible.
- [x] #2 Large groups initially render at most 60 items and provide current/total plus Show more and Show all.
- [x] #3 Search reveals matches in collapsed groups and clearing search restores prior explicit collapse state.
- [x] #4 Cards and table views preserve equivalent policy semantics and logical selection.
- [x] #5 Individual, Shift-range, group, cross-chunk, clear, selected export and selected delete work on filtered logical order.
- [x] #6 Bulk delete uses server eligibility, reports deleted/skipped/reasons, handles partial/all-blocked/failure, and preserves immutable blockers.
- [x] #7 Existing catalog API pagination is preserved; chunking remains client rendering only.
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implementing in shared worktree /home/mcamp/code/crystal-forge/TASK-433-policy-poam-workflows on branch TASK-433-policy-poam-workflows (based on dev @ c60b5799), per user decision to use one worktree/branch/MR for all TASK-433 phase subtasks rather than per-subtask worktrees.

## Implementation complete, verification partially run

**Server (bulk delete API):**
- `BulkDeletePoliciesRequest`/`Response`/`BulkDeleteSkippedPolicy` in api/models.rs.
- `bulk_delete_deployment_policies` query fn (queries/deployment_policies.rs): loops the existing per-policy transactional `delete_deployment_policy`, so one blocked policy never rolls back another eligible one. Reuses all existing eligibility/blocker logic verbatim (no duplicated deletion rules).
- `POST /api/v1/deployment-policies/bulk-delete` handler (admin-only, dedups ids, caps at 500), registered in bin/server.rs.
- 2 new DB integration tests in queries/deployment_policies.rs (`bulk_delete_deployment_policies_partial_success`, `bulk_delete_deployment_policies_reports_not_found`), run and passing against the repo's own isolated `db-only` Postgres (port 3042, migrated through 0232) via `CRYSTAL_FORGE_TEST_DATABASE_URL`.

**Client (catalog scaling UI):** policies.rs — BIG_GROUP=150/CHUNK=60 constants; per-group stable `key` (domain|grouping|label) with explicit collapse/shown-count override maps; search bypasses collapse+chunk without mutating stored state (clearing search restores prior shape); Cards/Table view toggle (new `PolicyRow` table component mirrors `PolicyCard` selection wiring); Shift-range + cross-chunk selection via a full (non-chunk-limited, `Rc`-shared) flat id order across expanded groups only; per-group Select/Deselect All; toolbar Clear; `BulkDeletePoliciesConfirm` modal + deleted/skipped-with-reason result banner, preserving skipped (blocked) policies as still-selected. IOMenu item renamed 'Select policies to export' -> 'Select multiple…'. New CSS: `.pol-group-toggle`, `.pol-group-head.is-collapsed`, `.pol-group-hidden-note`, `.pol-group-more`.

**Tests added:** 7 pure-logic unit tests (collapse/chunk defaults, flat-range inclusivity/direction/clamping, group-key uniqueness/stability) + 2 server DB integration tests, all passing.

**Verification run (recorded exact commands/results):**
- `SQLX_OFFLINE=true cargo check -p cf-server`: clean (only pre-existing warnings).
- `SQLX_OFFLINE=true cargo test -p cf-server --lib`: 1175 passed, 0 failed, 379 ignored (pre-existing DB-gated).
- New bulk-delete DB tests run explicitly against isolated `db-only` (not a shared/default instance): both pass.
- `cargo check --manifest-path packages/web-ui/Cargo.toml`: clean.
- `cargo test --manifest-path packages/web-ui/Cargo.toml`: 198 passed, 0 failed, 1 ignored.
- `cargo fmt --manifest-path packages/default/Cargo.toml -- --check`: clean, zero diffs.
- `cargo fmt --manifest-path packages/web-ui/Cargo.toml -- --check`: zero diffs in any file this subtask touched; remaining diffs are pre-existing baseline drift confirmed via `git stash` A/B comparison (policy_editor_modal.rs, environments/adapter.rs, views/compliance.rs, views/policies_api.rs, and the pre-existing `PolicyDrawer`/`security_policy()` formatting in policies.rs) — none introduced by this subtask.
- `nix build .#packages.x86_64-linux.web-ui --no-link`: succeeded (had to `git add` the new untracked policy_row.rs first — Nix flake source filtering only includes git-tracked files, a good learning for future subtasks: `git add` new files before any `nix build`).
- `nix build .#packages.x86_64-linux.server --no-link`: succeeded. Confirms SQLx offline metadata needs no refresh (bulk-delete query uses only dynamic `sqlx::query`/`query_scalar`, no new compile-time-checked `query!` macros were added).

**Not yet run (deferred — time-boxed this session):** `nix build .#checks.x86_64-linux.integration`, `.#checks.x86_64-linux.server-regressions`, `.#checks.x86_64-linux.web-ui`, `.#checks.x86_64-linux.ui-screenshots`, `.#checks.x86_64-linux.web-ui-reconciliation`, `nix flake check --keep-going`. No MR opened yet. Per the recorded TASK-433 plan, the full >60-policy browser workflow proof (parent AC #32) is explicitly finalized in TASK-433.9 (Phase 8), not this subtask, but the heavier nix checks above should still be run before this subtask is considered Review-ready.

Working tree: shared worktree /home/mcamp/code/crystal-forge/TASK-433-policy-poam-workflows, branch TASK-433-policy-poam-workflows. Nothing committed yet.

## Review state correction
The deferred-check note above is superseded. After the commit, all deferred checks were run successfully: `nix build .#checks.x86_64-linux.integration --no-link`, `nix build .#checks.x86_64-linux.server-regressions --no-link`, `nix build .#checks.x86_64-linux.web-ui --no-link`, `nix build .#checks.x86_64-linux.ui-screenshots --no-link`, `nix build .#checks.x86_64-linux.web-ui-reconciliation --no-link`, and `nix flake check --keep-going`. Commit `e80ebc56` is pushed and MR !318 is open.

## Remediation correction
The earlier implementation note describing per-policy transactions is superseded by commit e0d036db: bulk deletion now uses one transaction via `delete_deployment_policy_in_transaction`, commits expected blocked/not-found skips, and rolls back all eligible deletions on any unexpected error. The browser workflow is now extended in `checks/web-ui/tests/integration-test.js`; the DB coverage is 4 tests (partial, all-blocked, not-found, rollback-on-failure).
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
created: 2026-08-23 03:33
---
MR opened: https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/318

Phase 1 verification is complete. All targeted cargo tests, isolated bulk-delete DB tests, package builds, integration/server-regressions/web-ui/reconciliation/screenshot checks, and `nix flake check --keep-going` passed. The MR is ready for review.
---

created: 2026-08-23 03:45
---
Returned to In Progress after review of MR !318. Required fixes: include collapsed groups in Shift-range logical order; make bulk delete atomic or return/reconcile committed outcomes on unexpected failure; add >60-policy browser coverage; preserve collapsed-group selection controls; gate bulk delete by admin; make selection mode suppress conflicting edit/delete/revision actions; add all-blocked/failure/authorization/validation coverage.
---

created: 2026-08-23 04:34
---
Review remediation pushed in commit e0d036db and appended to MR !318.

Fixed P1/P2 findings: collapsed groups now remain in logical Shift-range order; bulk deletion is atomic with rollback on unexpected DB errors; collapsed groups expose group selection; bulk delete is admin-gated; selection mode hides conflicting row actions; collapsed hierarchy is compact; and the required >150-policy browser workflow was added.

New verification: isolated DB tests for partial success, all-blocked, not-found, and unexpected-failure rollback passed 4/4; web-ui tests 199 passed; server lib tests 1175 passed; web-ui Nix check passed; server/web-ui package builds passed; `nix flake check --keep-going` passed. Phase 2 remains not started.
---
<!-- COMMENTS:END -->

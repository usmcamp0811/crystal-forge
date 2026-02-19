---
id: TASK-65.1
title: Identity and RBAC data model plus migrations
status: Backlog
assignee: ["Codex 5.3"]
labels:
  - security
  - auth
  - rbac
  - database
  - backend
milestone: m-14
dependencies:
  - TASK-65
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Problem
There is no persisted model for users, external identities, role assignments, and session linkage.

Goal
Introduce persistent identity and RBAC schema plus domain models supporting single-tenant runtime with future multi-tenant-ready boundaries.

Non-Goals
- Full multi-tenant runtime behavior in v1.
- IAM admin UI beyond current scope.

Architectural Constraints
- Domain logic independent of UI types.
- Migrations are required for schema changes.
- No unwraps in production paths.

Verification Plan
- `nix develop -c cargo test --package default auth::models`
- `nix develop -c cargo test --package default auth::repository`
- `nix develop -c cargo clippy -- -D warnings`
- Manual: validate migration up and down in development DB.

Impact Areas
- Domain, Infrastructure, Database, API

Risk Level
- High
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Migration adds users, external identity mapping, role assignments, and required indexes and constraints
- [ ] #2 Roles are defined as Admin, Operator, and Viewer in domain model
- [ ] #3 External subject mapping model supports future tenant discriminator extension
- [ ] #4 Query and repository layer exposes explicit typed interfaces with no UI coupling
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Follow-up possibility: dedicated multi-tenant identity partitioning task.
<!-- SECTION:NOTES:END -->

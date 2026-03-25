---
id: TASK-214
title: Add systemConfiguration name field to decouple hostname from NixOS config path
status: Backlog
assignee: []
created_date: '2026-03-24'
labels:
  - backend
  - database
  - api
  - enhancement
  - system-management
dependencies: []
priority: medium
---

## Description

## Problem
Currently, systems must be named identically to their NixOS configuration name in the flake (e.g., if the flake has `nixosConfigurations.gray`, the system hostname must be `gray`). This creates unnecessary coupling between the system's network hostname and its NixOS configuration name.

The backend also stores/returns full flake attribute paths like `git+https://gitlab.com/user/dotfiles?rev=abc123#nixosConfigurations.test.gray` which makes UIs verbose.

## Goal
Decouple the system hostname from the NixOS configuration name by:
1. Adding a `system_configuration_name` field to the systems table
2. Updating the system add/edit form to let users specify which config to use
3. Using this field when building the flake attribute path for deployments

## Non-Goals
- Changing how flakes define configurations
- Auto-detecting available configurations (could be future enhancement)
- Modifying existing system records (migration will set config name = hostname)

## Scope
- Add `system_configuration_name` VARCHAR field to systems table (nullable for migration, defaults to hostname)
- Update system creation/edit API to accept and validate this field
- Update build queue and deployment logic to use the config name field
- Update frontend system form to include this input
- Migration to backfill existing systems with hostname as config name

## Architectural Constraints
- Maintain backward compatibility during migration
- Validate that the specified config name exists in the linked flake
- Keep hostname as the primary system identifier (not the config name)
- The pattern of hostname = config name should remain the recommended practice

## Verification Plan
- Create a system with hostname "prod-web-01" using config name "webserver"
- Verify build uses correct flake path: `flake#nixosConfigurations.webserver`
- Verify existing systems continue to work after migration
- Test validation when config name doesn't exist in flake

## Impact Areas
- `packages/server/src/db/schema.sql` - add column
- System models and API endpoints
- Build job creation logic
- Evaluation logic
- Frontend system form

## Risk Level
Medium - database schema change with migration required, touches deployment critical path

## Dependencies
None

## Acceptance Criteria
- [ ] #1 Systems table has `system_configuration_name` field
- [ ] #2 Migration adds field and backfills with hostname for existing systems
- [ ] #3 System creation API accepts optional `system_configuration_name` (defaults to hostname)
- [ ] #4 Build jobs use the config name field to construct flake attribute paths
- [ ] #5 Frontend form includes input for systemConfiguration name with hostname as default
- [ ] #6 Validation ensures config name exists in the linked flake
- [ ] #7 Existing systems continue to work unchanged after migration

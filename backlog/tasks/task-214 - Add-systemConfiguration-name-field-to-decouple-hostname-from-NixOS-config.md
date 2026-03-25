---
id: TASK-214
title: Add systemConfiguration name field to decouple hostname from NixOS config path
status: To Do
assignee: []
created_date: '2026-03-24'
updated_date: '2026-03-24'
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

<!-- SECTION:DESCRIPTION:BEGIN -->
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
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Systems table has `system_configuration_name` field
- [ ] #2 Migration adds field and backfills with hostname for existing systems
- [ ] #3 System creation API accepts optional `system_configuration_name` (defaults to hostname)
- [ ] #4 Build jobs use the config name field to construct flake attribute paths
- [ ] #5 Frontend form includes input for systemConfiguration name with hostname as default
- [ ] #6 Validation ensures config name exists in the linked flake
- [ ] #7 Existing systems continue to work unchanged after migration
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
### Database Migration Strategy

The migration must:
1. Add `system_configuration_name` column as NULLABLE VARCHAR
2. Backfill existing rows: `UPDATE systems SET system_configuration_name = hostname WHERE system_configuration_name IS NULL`
3. Consider adding NOT NULL constraint after backfill (optional - NULL could mean "use hostname")

### API Contract

System creation request:
```json
{
  "hostname": "prod-web-01",
  "system_configuration_name": "webserver",  // Optional, defaults to hostname
  "flake_id": "...",
  "environment_id": "..."
}
```

### Build Path Construction

Current logic (somewhere in build/evaluation):
```
build_target = f"{flake_url}#nixosConfigurations.{system.hostname}"
```

Should become:
```
config_name = system.system_configuration_name or system.hostname
build_target = f"{flake_url}#nixosConfigurations.{config_name}"
```

### Frontend UX

System form should show:
```
Hostname: [prod-web-01]
Configuration Name: [webserver] (defaults to hostname if empty)
```

With help text: "The NixOS configuration name from your flake. Leave empty to use the hostname."

### Validation Considerations

Option A: **Strict validation** (recommended)
- When system is created/updated, fetch flake evaluations
- Check if `nixosConfigurations.{config_name}` exists
- Reject if not found

Option B: **Lazy validation**
- Allow any config name
- Build/evaluation will fail if config doesn't exist
- Report error at build time

Recommend Option A for better UX, but requires flake evaluation data to be current.

### Edge Cases

1. **Config name with dots**: `nixosConfigurations.profiles.webserver.prod`
   - Current `extract_system_name()` takes last segment → `prod`
   - May need to store full config path after `nixosConfigurations.`

2. **Renaming config in flake**
   - System record becomes stale
   - Next evaluation/build will fail
   - Admin must update system record

3. **Null vs empty string**
   - NULL = use hostname (recommended)
   - Empty string = could be invalid, should validate as NULL

### Files to Modify

**Backend:**
- `packages/server/src/db/migrations/NNNN_add_system_configuration_name.sql`
- `packages/server/src/db/models/system.rs` (or equivalent)
- `packages/server/src/api/systems.rs` - creation/update endpoints
- Build job creation logic (wherever `nixosConfigurations.{hostname}` is constructed)

**Frontend:**
- `packages/web-ui/src/views/systems.rs` - system form
- `packages/web-ui/src/api/models.rs` - System model

### Testing Strategy

**Unit Tests:**
- API validation accepts valid config names
- API defaults to hostname when field is null/empty
- Migration backfills correctly

**Integration Tests:**
- Create system with custom config name → verify build path
- Create system without config name → verify defaults to hostname
- Existing systems work after migration

**Manual Verification:**
- Deploy to test instance
- Run migration
- Create new system with custom config name
- Trigger build and verify correct flake path in logs
<!-- SECTION:NOTES:END -->

---
id: TASK-292
title: Deprecate legacy database mode and implement builder API mode in NixOS module
status: To Do
assignee: []
created_date: '2026-05-08 02:59'
updated_date: '2026-05-08 03:50'
labels:
  - bug
  - dx
  - nixos-module
  - documentation
  - ui
dependencies:
  - TASK-293
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem Statement

The Crystal Forge NixOS module only supports legacy database mode (direct database access), which is deprecated. The recommended builder API mode is not supported by the module, forcing users to either:
1. Use deprecated legacy mode with warnings
2. Manually configure builder API mode outside the module

This creates deployment confusion and prevents users from following best practices.

## Current State

**NixOS Module (`modules/nixos/crystal-forge/default.nix`):**
- Only configures legacy database mode
- Does NOT set `CRYSTAL_FORGE__BUILDER__PRIVATE_KEY_PATH`
- Does NOT generate builder API keys
- Does NOT register builders with the server API
- Builder logs show: "⚠️  Starting builder in deprecated legacy direct-database mode"

**Missing from Module:**
- `build.api_mode` option
- Builder API key generation (similar to SSH key auto-generation)
- Builder registration flow
- Environment variable configuration for API mode
- Documentation for builder API mode setup

## Root Cause Analysis

The module was written before builder API mode existed. It needs to be updated to:
1. Support the new architecture as the default
2. Deprecate the old database mode
3. Provide migration path for existing deployments

## Desired Outcome

### Phase 1: Add Builder API Mode Support (This Task)

**NixOS Module Changes:**

1. **Add new options:**
   ```nix
   services.crystal-forge.build = {
     api_mode = lib.mkOption {
       type = lib.types.bool;
       default = true;
       description = "Use builder API mode (recommended). Set to false for legacy database mode (deprecated).";
     };
     
     api_key_file = lib.mkOption {
       type = lib.types.nullOr lib.types.path;
       default = null;
       description = "Path to builder API private key. If null, key will be auto-generated.";
     };
     
     server_url = lib.mkOption {
       type = lib.types.str;
       default = "http://127.0.0.1:3000";
       description = "Crystal Forge server URL for builder API mode";
     };
   };
   ```

2. **Auto-generate builder API keys in preStart:**
   ```bash
   # Similar to SSH key generation (lines 208-223)
   if [ ! -f /var/lib/crystal-forge/builder-api.key ]; then
     echo "Generating builder API key..."
     cf-keygen -f /var/lib/crystal-forge/builder-api.key
     chmod 0600 /var/lib/crystal-forge/builder-api.key
     chown crystal-forge:crystal-forge /var/lib/crystal-forge/builder-api.key
     echo "Builder API public key (register this in Crystal Forge UI):"
     cat /var/lib/crystal-forge/builder-api.key.pub
   fi
   ```

3. **Set environment variables for API mode:**
   ```nix
   environment = {
     CRYSTAL_FORGE__BUILDER__PRIVATE_KEY_PATH = 
       if cfg.build.api_mode then 
         cfg.build.api_key_file or "/var/lib/crystal-forge/builder-api.key"
       else null;
     CRYSTAL_FORGE__BUILDER__SERVER_URL = 
       if cfg.build.api_mode then cfg.build.server_url else null;
   };
   ```

4. **Emit deprecation warning for legacy mode:**
   ```nix
   warnings = lib.optional (!cfg.build.api_mode) 
     "Crystal Forge legacy database mode is deprecated. Please migrate to builder API mode by setting services.crystal-forge.build.api_mode = true";
   ```

5. **Fix SSH key permissions BEFORE first use:**
   ```bash
   # In preStart, BEFORE key generation check
   if [ -f /var/lib/crystal-forge/.ssh/id_ed25519 ]; then
     chmod 0600 /var/lib/crystal-forge/.ssh/id_ed25519
   fi
   ```

**Documentation Changes:**

1. **Update deployment guide:**
   - Add "Builder API Mode Setup" section
   - Document builder registration flow
   - Show how to get builder public key
   - Explain API mode vs legacy mode

2. **Add migration guide:**
   - Step-by-step: legacy → API mode
   - How to verify API mode is working
   - Troubleshooting common issues

3. **Update NixOS module reference:**
   - Document all new `build.*` options
   - Show example configurations
   - Mark legacy options as deprecated

4. **First-time deployment checklist:**
   - SSH key setup and permissions
   - Flake credentials configuration
   - Builder registration (API mode)
   - System registration
   - First flake sync verification

### Phase 2: Deprecation Timeline

- **Now (this task):** Add API mode support, keep legacy as option with warning
- **Next release:** Make API mode the default (`api_mode = true`)
- **Future release:** Remove legacy database mode entirely

## Acceptance Criteria

Module Implementation:
- [ ] #1 Add `build.api_mode` option (default: true)
- [ ] #2 Add `build.api_key_file` option with auto-generation fallback
- [ ] #3 Add `build.server_url` option for API endpoint
- [ ] #4 Auto-generate builder API key in preStart if not provided
- [ ] #5 Set `CRYSTAL_FORGE__BUILDER__PRIVATE_KEY_PATH` environment variable when api_mode enabled
- [ ] #6 Set `CRYSTAL_FORGE__BUILDER__SERVER_URL` environment variable when api_mode enabled
- [ ] #7 Emit deprecation warning when `api_mode = false`
- [ ] #8 Fix SSH key permissions to 0600 in preStart before any key operations

Documentation:
- [ ] #9 Add "Builder API Mode" section to deployment guide
- [ ] #10 Document builder registration flow with screenshots
- [ ] #11 Add migration guide: legacy database mode → API mode
- [ ] #12 Update NixOS module reference with new `build.*` options
- [ ] #13 Mark legacy mode options as deprecated in docs
- [ ] #14 Add first-time deployment checklist with API mode steps

Verification:
- [ ] #15 Test fresh deployment with `api_mode = true` (default)
- [ ] #16 Test migration from legacy mode to API mode
- [ ] #17 Verify builder registers with server API on first start
- [ ] #18 Verify auto-generated keys have correct permissions (0600)
- [ ] #19 Verify deprecation warning appears when `api_mode = false`

## Out of Scope

- UI improvements (first-run wizard) - separate task
- Builder diagnostics improvements - separate task
- Automatic builder registration via module - requires server API changes

## Implementation Notes

**Key Generation Tool:**
The module needs access to `cf-keygen` binary. Verify this is available in the package:
```nix
${pkgs.crystal-forge.default.server}/bin/cf-keygen
```

**Builder Registration:**
For now, users must manually register the builder public key via UI. Future enhancement: auto-register via server API.

**Backward Compatibility:**
Existing deployments with legacy mode continue to work. They just get a warning encouraging migration.

## Files to Modify

- `modules/nixos/crystal-forge/default.nix` - Add API mode support
- `docs/deployment.md` (or equivalent) - Add builder API mode docs
- `docs/nixos-module.md` (or equivalent) - Document new options
- `docs/migration-legacy-to-api.md` - New migration guide
- `docs/troubleshooting.md` - Add builder API mode troubleshooting

## Definition of Done

- NixOS module supports builder API mode as default
- Fresh deployments use API mode without manual configuration
- Documentation covers all new options and migration path
- Existing deployments show deprecation warning for legacy mode
- Tests verify API mode works end-to-end
- Builder public key is displayed on first activation for easy registration
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 NixOS module supports builder API mode with build.api_mode option (default: true)
- [ ] #2 Module auto-generates builder API key at /var/lib/crystal-forge/builder-api.key if not provided
- [ ] #3 Module sets CRYSTAL_FORGE__BUILDER__PRIVATE_KEY_PATH environment variable when api_mode enabled
- [ ] #4 Builder registers with server API on first start in API mode
- [ ] #5 Module emits deprecation warning when build.api_mode = false (legacy database mode)
- [ ] #6 SSH key permissions set to 0600 in preStart before first use
- [ ] #7 Builder logs explain why view_buildable_derivations is empty (at least 3 scenarios)
- [ ] #8 Documentation includes builder API mode setup and legacy deprecation notice
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
## Implementation Blocker Found

The cf-keygen binary has an interactive confirmation prompt that will block in automated scripts.

Required Fix: Add -y or --force flag to cf-keygen to skip confirmation for automated use.

Workaround: Use echo y | cf-keygen -f ... in preStart script.

See cf-keygen.rs lines 84-92 for the blocking prompt code.
<!-- SECTION:NOTES:END -->

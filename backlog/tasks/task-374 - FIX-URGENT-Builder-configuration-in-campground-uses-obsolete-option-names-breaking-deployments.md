---
id: TASK-374
title: >-
  FIX URGENT: Builder configuration in campground uses obsolete option names
  breaking deployments
status: Backlog
assignee: []
created_date: '2026-06-27 16:04'
labels: []
dependencies: []
priority: high
ordinal: 321000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

The campground deployment configurations (webb and reckless) are using obsolete builder configuration option names that no longer exist in the Crystal Forge NixOS module, even on the TASK-282 branch.

**Current broken config (webb/reckless):**
```nix
builder = {
  enable_api_mode = true;  # ❌ Does not exist
  builder_id = "...";      # ❌ Does not exist  
  server_url = "...";      # ✅ Exists but in wrong location
  private_key_vault_field = "...";  # ❌ Does not exist
};
```

**Correct config format:**
```nix
build = {
  enable = true;
  api_mode = true;  # ✅ Correct option name
  server_url = "...";  # ✅ In build block
  api_key_file = null;  # ✅ Auto-generates if not specified
  # No builder_id - auto-registered
  # No private_key_vault_field - use api_key_file instead
};
```

## Impact

- **webb**: Builder fails to start with UUID parsing errors and database connection failures
- **reckless**: Likely using wrong configuration format
- Deployments are blocked
- Users are confused about correct configuration

## Root Cause

TASK-292 changed the option names and structure but existing campground configs were never updated to match. The old `builder` block with `enable_api_mode` was replaced with `build.api_mode`.

## Discovered Issues

1. `enable_api_mode` → Should be `api_mode` 
2. `builder.` block → Options moved to `build.` block
3. `builder_id` → No longer exists (auto-registered via public key)
4. `private_key_vault_field` → Should use `api_key_file` instead
5. Even with `api_mode = true`, builder still tries database connection (potential module bug)

## Immediate Fix Needed

Update campground system configs:
- `/home/mcamp/code/campground/systems/x86_64-linux/webb/default.nix`
- `/home/mcamp/code/campground/systems/x86_64-linux/reckless/default.nix`

Use correct option names per TASK-292 implementation.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 webb builder starts successfully with correct api_mode configuration
- [ ] #2 reckless builder configuration updated to use correct option names
- [ ] #3 Builder successfully registers with server using auto-generated API key
- [ ] #4 No database connection attempts when api_mode = true (if module bug exists, file separate task)
- [ ] #5 Documentation or migration notes added to prevent future confusion
<!-- AC:END -->

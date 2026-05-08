---
id: TASK-292
title: >-
  Fix builder "no work available" confusion: improve NixOS module defaults and
  diagnostics
status: Backlog
assignee: []
created_date: '2026-05-08 02:59'
updated_date: '2026-05-08 03:10'
labels:
  - bug
  - dx
  - nixos-module
  - documentation
  - ui
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

Users deploying Crystal Forge encounter confusing builder behavior:

1. **Builder API mode not supported by NixOS module:**
   - Module only configures legacy database mode
   - `CRYSTAL_FORGE__BUILDER__PRIVATE_KEY_PATH` not set
   - Builder shows deprecation warning: "Starting builder in deprecated legacy direct-database mode"
   - No way to enable builder API mode via module options

2. **Silent failures confuse users:**
   - Builder shows `Worker 0: Idle` with no explanation
   - `view_buildable_derivations` returns 0 rows with no diagnostic info
   - Flake sync failures are silent (SSH key perms, missing credentials)
   - `cf_agent_enabled` eval failures cause derivations to be invisibly filtered out

3. **First-time deployment gotchas:**
   - SSH keys auto-generated with 0755 perms (SSH requires 0600)
   - tmpfiles fixes perms on reboot but not on first activation
   - No setup checklist or wizard
   - Build scope defaults can filter out all work

## Current State

The NixOS module:
- Only supports legacy database mode (deprecated)
- Has no builder API mode support (the recommended architecture)
- Auto-generates SSH keys with wrong permissions
- Provides no diagnostic output when builder is idle
- Has no first-run validation

## Desired Outcome

1. **Deprecate legacy database mode, migrate to builder API mode:**
   - Add `build.api_mode` option (default: true)
   - Auto-generate builder API key if not provided (like SSH keys)
   - Set `CRYSTAL_FORGE__BUILDER__PRIVATE_KEY_PATH` environment variable
   - Register builder with server on first start
   - Emit deprecation warnings if `api_mode = false`

2. **NixOS module hardening:**
   - Fix SSH key permissions in preStart (before services need them)
   - Validate required setup exists (flakes configured, credentials present)
   - Ensure tmpfiles directories exist before service start

3. **Builder diagnostics:**
   - Log WHY `view_buildable_derivations` is empty:
     - "No flakes configured"
     - "All derivations filtered: cf_agent_enabled=NULL/false"
     - "Build scope restricts to CF systems but none qualified"
   - Make these visible in INFO or WARN level logs

4. **UI/UX improvements:**
   - First-run setup wizard or checklist
   - Dashboard warnings for missing critical setup
   - Builder page explains idle state (config missing vs no work)

5. **Documentation:**
   - Deployment checklist with correct sequence
   - Troubleshooting: "builder idle but I expected work"
   - Migration guide: legacy → API mode
   - Deprecation timeline for database mode

## Migration Path

For existing deployments using legacy database mode:
- Phase 1: Make API mode available, keep legacy as deprecated default
- Phase 2: Switch default to API mode, warn on legacy mode
- Phase 3: Remove legacy database mode entirely
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

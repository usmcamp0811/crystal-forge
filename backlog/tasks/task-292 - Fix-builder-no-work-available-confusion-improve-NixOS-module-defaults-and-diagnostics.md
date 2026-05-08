---
id: TASK-292
title: >-
  Fix builder "no work available" confusion: improve NixOS module defaults and
  diagnostics
status: Backlog
assignee: []
created_date: '2026-05-08 02:59'
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

Users deploying Crystal Forge for the first time encounter a confusing situation where:
1. The builder starts successfully and shows `Worker 0: Idle`
2. Logs show `SELECT ... FROM view_buildable_derivations` returning 0 rows
3. No error messages explain WHY there's no work
4. The UI doesn't indicate what's missing (flake not configured, SSH keys not set up, etc.)

This creates the impression that the builder is "broken" when the actual root cause is:
- No flake configured yet
- Flake sync failing due to SSH key permissions (documented gotcha: auto-generated key has 0755 perms, SSH requires 0600)
- `cf_agent_enabled` eval failing silently, causing derivations to be filtered out
- Build scope set to "Only Crystal Forge systems" but no systems have `cf_agent_enabled=true`

## Current State

The NixOS module has several known issues documented in the deployment setup guide:
- SSH key auto-generated with 0755 permissions (SSH refuses keys > 0600)
- tmpfiles rules fix this on boot BUT not on first activation
- Flake evaluation failures are silent
- Builder logs don't explain why `view_buildable_derivations` is empty
- No first-run checklist or setup wizard in UI

## Desired Outcome

1. **NixOS module improvements:**
   - Fix SSH key permissions DURING preStart, not just via tmpfiles
   - Add validation checks that warn/fail if flake credentials aren't configured
   - Ensure tmpfiles directories exist before services start (first activation race condition)

2. **Builder diagnostics improvements:**
   - When `view_buildable_derivations` returns 0 rows, log WHY (debug mode):
     - "No flakes configured"
     - "No derivations in pending state"
     - "All pending derivations filtered: cf_agent_enabled=NULL/false"
     - "Build scope restricts to CF systems, but no systems have cf_agent_enabled=true"
   
3. **UI/UX improvements:**
   - First-run setup checklist or wizard
   - Dashboard shows missing setup items (no flakes, SSH keys not configured, no systems registered)
   - Builder status page explains when idle due to configuration vs actually no work

4. **Documentation improvements:**
   - Add "Deployment Checklist" to docs
   - Sequence setup steps correctly (SSH keys → flake credentials → system registration → first sync)
   - Add troubleshooting section with "builder idle but I expected builds" diagnostic steps
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 NixOS module sets SSH key permissions to 0600 in preStart script before first use
- [ ] #2 NixOS module validates required directories exist before services start (no NAMESPACE errors on first activation)
- [ ] #3 Builder logs explain why view_buildable_derivations is empty when in debug mode (at least 3 common scenarios covered)
- [ ] #4 Documentation includes a First Deployment Checklist with sequenced setup steps
- [ ] #5 UI dashboard shows actionable warnings when critical setup is missing (no flakes configured, no valid credentials, etc.)
<!-- AC:END -->

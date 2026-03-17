---
id: TASK-194
title: >-
  Remove pre-populated data from server-stack up (keep clean for
  server-stack-mock only)
status: Review
assignee: []
created_date: '2026-03-17 03:13'
updated_date: '2026-03-17 03:28'
labels:
  - devops
  - configuration
  - testing
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

`server-stack up` currently pre-populates the database with mock data (environments, systems, flakes) even though it's meant to be a clean production-like deployment. This violates the principle that only `server-stack-mock` should have pre-seeded data.

## Current Behavior

The shared `configTemplate` in `packages/devScripts/default.nix` (lines 88-148) includes:
- Mock environment "mockenv" (lines 123-130)
- Mock system "test.gray" (lines 131-136)  
- Mock flake "dotfiles" (lines 137-147)

This config is used by ALL stacks including `server-only` (server-stack).

## Goal

Ensure `server-stack up` behaves like a production deployment with a clean database, while `server-stack-mock up` continues to provide pre-populated mock data for development/demos.

## Non-Goals

- This task does NOT change the behavior of `server-stack-mock` (it should keep mock data)
- This task does NOT change database schema or migrations
- This task does NOT affect the NixOS module configuration
- This task does NOT change the web UI or API behavior

## Expected Behavior

- `server-stack up`: Clean database, NO pre-populated environments/systems/flakes (same as NixOS module deployment)
- `server-stack-mock up`: Pre-populated with mock data for development/demo
- `full-stack up`: Clean database (agent can self-register with API key workflow)
- `oidc-stack up`: Clean database

## Solution

Create two config template variants:
1. `configTemplateClean` - No environments/systems/flakes arrays (production-like)
2. `configTemplateMock` - Includes mock data (development/demo)

Modify `generateConfig` to accept a parameter for which template to use, or create separate generator functions.

Use clean template for server-module, mock template only for mock-execution-module.

## Architectural Constraints

- Must maintain backward compatibility with `server-stack-mock up`
- Config generation must remain deterministic
- HOSTNAME_PLACEHOLDER replacement must work for both templates
- Builder and cache key generation must continue to work
- Process-compose health checks must continue to work

## Impact Areas

- `packages/devScripts/default.nix` - Config template splitting
- All process-compose stacks (server-stack, server-stack-mock, full-stack, oidc-stack)
- Developer workflow documentation (if any references mock data availability)

## Risk Level

Low-Medium - Configuration change with clear rollback path (revert commit), but affects developer workflow

## Verification Plan

- Tier 0:
  - `nix flake check` (ensure Nix evaluation succeeds)
  - Verify config template syntax is valid TOML
- Tier 1:
  - `server-stack up` → verify database is empty (no environments/systems/flakes)
  - `server-stack-mock up` → verify mock data exists
  - `full-stack up` → verify clean start
  - `oidc-stack up` → verify clean start
  - Test basic workflows in each stack (create environment, register system, add flake)
- Tier 2:
  - Not required (config-only change)
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 `server-stack up` starts with empty environments, systems, and flakes tables
- [ ] #2 `server-stack-mock up` continues to have pre-populated mock data
- [x] #3 `full-stack up` behavior is explicitly decided (clean or with test.gray)
- [ ] #4 No regressions in existing functionality
- [ ] #5 All process-compose stacks continue to work
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
LOCK: Claude (OpenCode) on gray in ~/code/crystal-forge/TASK-194-clean-server-stack

## Progress Update (2026-03-17)

### Implementation Complete

Created config template split:
1. configTemplateClean - No pre-populated data (lines 97-139)
2. configTemplateMock - Includes mockenv, test.gray, dotfiles (lines 142-203)
3. configTemplate - Aliased to clean template for backward compat (line 206)

Created helper function approach:
- makeGenerateConfig - Accepts template parameter (lines 31-78)
- generateConfig - Uses clean template (line 84)
- generateConfigMock - Uses mock template (line 87)

Created mock runtime variants:
- runServerMock - Uses generateConfigMock (lines 364-378)
- runBuilderMock - Uses generateConfigMock (lines 381-400)

Updated mock-execution-module:
- Uses runServerMock instead of runServer (line 666)
- Uses runBuilderMock instead of runBuilder (line 676)

### Verification Status
- Tier 0: Nix evaluation successful, devScripts package builds
- Tier 1: Pending manual stack testing

### Criterion 3 Decision

full-stack uses server-module which uses runServer -> generateConfig -> configTemplateClean.
Therefore full-stack will start with a CLEAN database (no pre-populated data).
This allows the agent to self-register using the API key workflow, which is the intended production-like behavior.

### Code Review Summary

Change Flow Analysis:
1. server-stack -> server-only -> server-module -> runServer -> generateConfig -> configTemplateClean ✅
2. server-stack-mock -> server-stack-mock -> server-module + mock-execution-module (overrides) -> runServerMock -> generateConfigMock -> configTemplateMock ✅
3. full-stack -> full-stack -> server-module -> runServer -> generateConfig -> configTemplateClean ✅

All architectural constraints satisfied:
- Backward compat maintained (configTemplate alias) ✅
- Config generation deterministic (same inputs = same outputs) ✅
- HOSTNAME_PLACEHOLDER replacement works (same sed logic in both) ✅
- Builder and cache key generation unchanged ✅
- Process-compose health checks unchanged ✅

Static verification complete. Runtime testing required for final acceptance.

### Merge Request Created

MR !170: https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/170

Branch: TASK-194-clean-server-stack
Target: dev

The implementation is complete and ready for review. Runtime verification is recommended during review to confirm the database behavior matches expectations.
<!-- SECTION:NOTES:END -->

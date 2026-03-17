---
id: TASK-194
title: >-
  Remove pre-populated data from server-stack up (keep clean for
  server-stack-mock only)
status: To Do
assignee: []
created_date: '2026-03-17 03:13'
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

## Expected Behavior

- `server-stack up`: Clean database, NO pre-populated environments/systems/flakes (same as NixOS module deployment)
- `server-stack-mock up`: Pre-populated with mock data for development/demo
- `full-stack up`: Clean (or decide if agent needs test.gray pre-registered)
- `oidc-stack up`: Clean

## Solution

Create two config templates:
1. `configTemplateClean` - No environments/systems/flakes arrays (production-like)
2. `configTemplateMock` - Includes mock data (development/demo)

Use clean template for server-module, mock template only for mock-execution-module.

## Acceptance Criteria

- [ ] `server-stack up` starts with empty environments, systems, and flakes tables
- [ ] `server-stack-mock up` continues to have pre-populated mock data
- [ ] `full-stack up` behavior is explicitly decided (clean or with test.gray)
- [ ] No regressions in existing functionality
- [ ] All process-compose stacks continue to work

## Implementation Notes

Modify `packages/devScripts/default.nix`:
- Create two config templates (clean and mock variants)
- Use clean template for normal server-module
- Use mock template in mock-execution-module
- Ensure HOSTNAME_PLACEHOLDER replacement works for both
<!-- SECTION:DESCRIPTION:END -->

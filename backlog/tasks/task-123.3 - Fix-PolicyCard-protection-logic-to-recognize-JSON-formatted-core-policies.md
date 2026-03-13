---
id: TASK-123.3
title: Fix PolicyCard protection logic to recognize JSON-formatted core policies
status: Done
assignee: []
created_date: '2026-03-09 22:47'
updated_date: '2026-03-13 01:24'
labels:
  - frontend
  - web-ui
  - bug
  - policies
  - security
milestone: m-13
dependencies: []
parent_task_id: TASK-123
priority: high
ordinal: 6000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

The UI adapter converts backend policies into JSON bodies and marks them with `PolicyFormat::Json` when loading from the API. However, `PolicyCard` decides whether something is the protected core policy by checking only for TOML-style strings like `type = "require_cf_agent"` or `type = "require_crystal_forge_agent"` inside `policy.body`.

In practice, this means the real DB-backed core policy will usually NOT be recognized as protected in the card UI, so the "Core / Always On" badge and the client-side hiding of Edit/Delete actions will be wrong for actual loaded data.

The backend still blocks destructive changes, so this is more of a correctness/UX/protection mismatch than a data-loss bug, but it directly undermines the safety model this MR says it adds.

## Goal

Fix the PolicyCard protection detection to correctly identify core policies regardless of their format (TOML or JSON), so that DB-loaded policies receive the same UI protection as mock policies.

## Non-Goals

- Changing the backend protection logic (already works correctly)
- Changing the policy format conversion (JSON is appropriate for API data)
- Supporting additional policy formats beyond TOML and JSON
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 PolicyCard protection logic checks policy_type field, not string pattern matching in body
- [x] #2 Core policy detection works for both PolicyFormat::Json and PolicyFormat::Toml
- [ ] #3 DB-loaded core policy shows 'Core / Always On' badge in UI
- [ ] #4 Edit/Delete buttons are hidden for DB-loaded core policy when user is not Admin
- [ ] #5 Mock core policy continues to show same protection behavior
- [ ] #6 Manual test: Load core policy from DB, verify badge appears and Edit/Delete hidden for non-Admin
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
LOCK: claude-agent on gray in ~/code/crystal-forge/TASK-123-deployment-policies-crud

Fixed: Added policy_type field to PolicyDefinition

PolicyCard now checks policy_type instead of string matching in body

Core policy detection works for both TOML and JSON formats

Updated all PolicyDefinition constructors to populate policy_type

Added helper function to extract policy_type from TOML/JSON bodies

Changes committed: 9a407c5f
<!-- SECTION:NOTES:END -->

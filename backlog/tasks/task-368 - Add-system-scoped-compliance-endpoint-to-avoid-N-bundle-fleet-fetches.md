---
id: TASK-368
title: Add system-scoped compliance endpoint to avoid N-bundle fleet fetches
status: Done
assignee: []
created_date: '2026-06-25 01:26'
updated_date: '2026-06-25 01:29'
labels:
  - backend
  - api
  - compliance
  - performance
milestone: 'm-19: Design Parity Existing Surfaces'
dependencies: []
priority: medium
ordinal: 320000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

The System Detail Compliance tab currently fetches compliance data by:
1. Fetching all compliance bundles (GET /api/v1/compliance/bundles)
2. For each bundle, fetching all system rollups (GET /api/v1/compliance/bundles/:id/systems)
3. Filtering client-side to find bundles applicable to the current system

This scales with the number of bundles × fleet size rather than the compliance data for one system. Even with concurrent fetching (TASK-356 fix), this is wasteful and slow for large fleets.

## Goal

Add a system-scoped backend endpoint:
```
GET /api/v1/systems/:system_id/compliance
```

Returns only the compliance bundles applicable to the specified system, with their system-specific rollups included.

Response schema:
```json
{
  "bundles": [
    {
      "bundle": { ComplianceBundleSummary },
      "rollup": { ComplianceSystemRollup }
    }
  ]
}
```

## Non-Goals

- Changing the Compliance view (fleet-wide) data flow
- Adding new compliance features
- Changing authorization model

## Acceptance Criteria

- New endpoint returns only bundles where the system appears in the bundle's system list
- Response includes bundle summary and system-specific rollup in one request
- TASK-356 ComplianceTab refactored to use new endpoint (remove N-bundle fetch loop)
- Existing authorization preserved (users see only bundles they have access to)
- Backend query efficient (single DB query or minimal joins)
- API documented in OpenAPI schema if applicable
<!-- SECTION:DESCRIPTION:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Task absorbed into TASK-356 - implementing backend fix directly instead of deferring.
<!-- SECTION:NOTES:END -->

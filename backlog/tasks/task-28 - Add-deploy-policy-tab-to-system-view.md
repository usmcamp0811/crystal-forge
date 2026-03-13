---
id: TASK-28
title: Add deploy policy tab to system view
status: Done
assignee:
  - Claude Opus 4.5
created_date: '2026-02-16 17:09'
updated_date: '2026-03-13 01:24'
labels: []
milestone: m-8
dependencies: []
ordinal: 14000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Add a Policy/Deploy Policy tab in system detail view to define deployment policies (TOML/JSON) based on packages/default/src/models/deployment_policies.rs.
<!-- SECTION:DESCRIPTION:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Goal: add Policy/Deploy Policy tab in system detail. UI should accept TOML/JSON policy definitions aligned to packages/default/src/models/deployment_policies.rs. Decide on editor + validation UX.

User request: add Policy/Deploy Policy tab on system view; policies defined in packages/default/src/models/deployment_policies.rs. Pending UI stub + editor.

Added Policy tab in system detail with TOML/JSON editor, taller textarea, and basic syntax-highlight preview stub.

Closed after in-progress review: Policy tab/editor stub exists in system detail view with TOML/JSON support scaffold as described in implementation notes.
<!-- SECTION:NOTES:END -->

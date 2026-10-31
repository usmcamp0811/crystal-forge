---
id: TASK-510
title: >-
  Persist managed registration address, reachability, tags and notes shown by
  the approved system forms
status: Backlog
assignee: []
created_date: '2026-10-31T21:36:00Z'
labels: []
dependencies: []
priority: medium
type: feature
ordinal: 561000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The approved Add System and Edit System designs (docs/design/CrystalForge/components/AddSystemModal.jsx, EditSystemModal.jsx) include an FQDN/address field and a "Reachable by the server directly" toggle for managed systems, free-form tags for all modes, and a description/notes field. These fields predate the management-mode design commit 18b47eed and were already absent or placeholder-only in production before TASK-508. The create API does not accept fqdn/reachability, and no API persists tags or notes, so TASK-508 renders tags/notes as explicitly unsupported rather than pretending to save them. Operators cannot group or filter by tags as the Systems list design intends.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Managed registration accepts and persists FQDN/address and reachability with server validation
- [ ] #2 Tags (all modes) and description/notes persist through create and edit APIs with RBAC and audit
- [ ] #3 Add/Edit forms and Systems tag filter use the persisted values per the approved design
- [ ] #4 Tests and knowledge docs cover the new fields
<!-- AC:END -->

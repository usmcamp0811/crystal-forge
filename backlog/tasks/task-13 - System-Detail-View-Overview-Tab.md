---
id: TASK-13
title: System Detail View - Overview Tab
status: To Do
assignee: []
created_date: '2026-02-05 14:25'
labels:
  - ui
  - views
  - systems
dependencies:
  - TASK-8.7
  - TASK-8.8
  - TASK-9
  - TASK-15
priority: high
milestone: m-3
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Build system detail page with overview information using Tailwind CSS.

Steps:
1. Create src/ui/views/system_detail.rs with tab layout
2. Accept system ID from Dioxus Router URL params
3. Implement Overview tab showing:
   - Current config (commit hash, store path, NixOS version)
   - Hardware info (CPU brand/cores, memory GB, uptime)
   - Network info (primary IP, MAC, gateway)
   - Health status with colored visual indicator
   - Deployment status badge (up-to-date/behind/pending)
   - Security: TPM, Secure Boot, FIPS status
4. Add action buttons: Deploy, Rollback, Force Sync
5. Fetch data from MockClient or real API (GET /api/v1/systems/:id from TASK-15)
6. Handle action button clicks (POST /api/v1/systems/:id/deploy, /rollback from TASK-15)
7. Show loading/error states for data fetching and action results
8. Style with Tailwind dark theme (info sections in cards, status badges)

Architecture notes:
- Action buttons require TASK-15 (Systems API endpoints) to actually work
- Use MockClient for initial development, switch to real client when API is ready
- Tab component should be reusable (future tabs: Deployments, CVEs, Builds)
- Data maps to SystemDetail API DTO (TASK-8.5), sourced from System + SystemState models

Expected: Overview answers "is it up to date? why not?" at a glance
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Overview tab complete
- [ ] #2 All info displayed
- [ ] #3 Action buttons work
- [ ] #4 Answers key questions
<!-- AC:END -->

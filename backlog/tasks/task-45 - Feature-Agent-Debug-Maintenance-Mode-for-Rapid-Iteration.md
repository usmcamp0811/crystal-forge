---
id: TASK-45
title: 'Feature: Agent Debug/Maintenance Mode for Rapid Iteration'
status: Backlog
assignee: ["KimiK2.5"]
created_date: '2026-02-17 16:58'
updated_date: '2026-02-19 03:39'
labels:
  - feature
  - agent
  - deployment
  - ui
  - maintenance
dependencies: []
priority: high
milestone: m-4
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Add ability to put systems into debug mode from UI, which increases agent polling frequency (15min → 30sec/1min) and pauses automatic deployments, allowing rapid iteration on config changes via manual deployments only.

## Context

Crystal Forge uses a pull-based architecture where agents poll the server for updates. This avoids firewall complexity from push-based notifications, but makes rapid iteration difficult when the default polling interval is ~15 minutes.

## Feature Requirements

### 1. Debug Mode Toggle (UI)
- Add debug mode toggle to system detail view in web UI
- Show visual indicator when system is in debug mode (badge, color change)
- Display current polling frequency for each system

### 2. Polling Frequency Change
- Default polling interval: ~15 minutes
- Debug mode polling interval: 30 seconds or 1 minute (configurable)
- Agent should check for debug mode status on each poll
- Agent should adjust its sleep interval based on server response

### 3. Automatic Deployment Pause
- When system is in debug mode, automatic deployments are suspended
- Only manual deployments are allowed during debug mode
- Existing auto-deploy policies are temporarily ignored
- When debug mode is disabled, normal auto-deploy behavior resumes

### 4. Database Schema
- Add `debug_mode` boolean to systems table
- Add `debug_poll_interval_seconds` integer (nullable, defaults to 60)
- Add `debug_mode_enabled_at` timestamp (nullable)

### 5. API Endpoints
- PUT /api/systems/{id}/debug-mode - enable/disable debug mode
- GET /api/systems/{id} - include debug mode fields in response
- Agent poll endpoint should return debug mode status + poll interval

### 6. Agent Behavior
- Agent receives debug mode status from server during each poll
- If debug mode enabled, use short poll interval (30s-60s)
- If debug mode disabled, use default interval (15min)
- Log when entering/exiting debug mode

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 UI has toggle to enable/disable debug mode per system
- [ ] #2 Visual indicator shows when system is in debug mode
- [ ] #3 Agent polls every 30s-60s when in debug mode
- [ ] #4 Agent polls every ~15min when NOT in debug mode
- [ ] #5 Automatic deployments are blocked during debug mode
- [ ] #6 Manual deployments work during debug mode
- [ ] #7 API endpoints exist for debug mode management
- [ ] #8 Database schema supports debug mode state
- [ ] #9 Debug mode state persists until explicitly disabled

## Dependencies

- Requires agent polling mechanism (exists)
- Requires system management UI (TASK-33, TASK-34)
- Requires manual deploy functionality (exists)

## Technical Notes

- Consider adding a timeout/expiration for debug mode (e.g., auto-disable after 2 hours)
- Consider audit logging when debug mode is enabled/disabled
- Consider adding debug mode to system list view for quick toggle
<!-- SECTION:DESCRIPTION:END -->
<!-- AC:END -->

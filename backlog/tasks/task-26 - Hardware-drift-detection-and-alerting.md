---
id: TASK-26
title: Hardware drift detection and alerting
status: To Do
assignee: []
created_date: '2026-02-16 05:03'
labels:
  - feature
  - security
  - server
  - web-ui
dependencies: []
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Track baseline hardware from first system heartbeat and alert when hardware changes. This could indicate system replacement, tampering, or misconfiguration.

## Requirements
1. Store baseline hardware snapshot from first heartbeat (CPU, memory, board serial, etc.)
2. Compare subsequent heartbeats to detect drift
3. API should return both current and baseline hardware info
4. UI should display warnings/alerts highlighting specific changes
5. Consider which fields should trigger alerts vs info notices

## Technical Notes
- May need new DB table or fields for baseline storage
- Server-side comparison logic needed
- UI changes to SystemDetailView to show drift warnings

## Context
Discovered while building the system detail page - currently showing hardware info from latest heartbeat only.
<!-- SECTION:DESCRIPTION:END -->

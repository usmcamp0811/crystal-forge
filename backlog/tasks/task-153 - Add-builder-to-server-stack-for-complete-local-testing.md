---
id: TASK-153
title: Add builder to server-stack for complete local testing
status: Review
assignee: []
created_date: '2026-03-01 23:31'
labels:
  - devtools
  - process-compose
  - testing
dependencies: []
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

After implementing automatic build job creation (TASK-143), there's no easy way to test the complete pipeline locally. The server-stack process-compose only runs PostgreSQL + Server, so build jobs get created but never processed.

## Goal

Add the builder process to server-stack so developers can test the complete flow:
1. Commit evaluation → build job creation
2. Builder picks up jobs from queue
3. Builder executes builds
4. Results stored in database

## Scope

- Add runBuilder script to devScripts (similar to runServer, runAgent)
- Create builder-module for process-compose
- Add builder-module to server-only configuration
- Update help text to mention builder is included
- Builder runs in legacy database mode (no API mode needed for local dev)
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 runBuilder script created in devScripts
- [ ] #2 builder-module added to process-compose configuration
- [ ] #3 builder-module added to server-only stack
- [ ] #4 server-stack help text updated to mention builder
- [ ] #5 nix build .#devScripts.server-only succeeds
- [ ] #6 server-stack up starts all three processes (db, server, builder)
- [ ] #7 Builder successfully claims and processes build jobs
<!-- AC:END -->

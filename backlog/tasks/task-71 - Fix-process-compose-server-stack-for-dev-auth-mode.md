---
id: TASK-71
title: Fix process-compose server-stack for dev auth mode
status: Backlog
assignee: []
created_date: '2026-02-20 14:27'
labels:
  - devex
  - infra
  - auth
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Problem: server-stack up fails because it uses the packaged release binary which rejects AUTH_MODE=dev due to production guard.

Goal: Allow process-compose server-stack to use dev auth mode for local testing.

Solution Options:
1. Add --dev flag to run-server that uses cargo run instead of packaged binary
2. Create separate dev-server-stack profile that uses debug build
3. Modify production guard to check for specific env var override (NOT RECOMMENDED - defeats the purpose)

Recommended: Option 1 - modify run-server to support --dev flag like run-agent does.

Acceptance Criteria:
- server-stack up works with AUTH_MODE=dev in development
- Production builds still reject AUTH_MODE=dev (guard remains effective)
- Documentation updated to explain dev vs release server execution
<!-- SECTION:DESCRIPTION:END -->

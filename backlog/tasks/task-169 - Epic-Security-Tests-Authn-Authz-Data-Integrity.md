---
id: TASK-169
title: 'Epic: Security Tests - Authn/Authz, Data Integrity'
status: Backlog
assignee: []
created_date: '2026-03-04 03:09'
labels: []
milestone: m-15
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem
Security tests needed to verify authentication, authorization, and data integrity boundaries.

## Goals
- Builders cannot impersonate other builders
- Agents can only update their own system state
- Workers cannot write arbitrary DB fields
- Replay protection / idempotency keys
- Log injection / size limits

## Scope
Authz boundaries, data integrity, input validation.

## Release Blockers
- Basic authz boundaries

## Acceptance Criteria
- [ ] Builders cannot impersonate other builders (auth token for A cannot claim for B)
- [ ] Agents can only update their own system state (POST for other system rejected)
- [ ] Workers cannot write arbitrary DB fields (API enforces allowed updates)
- [ ] Replay protection / idempotency keys (replays return same result)
- [ ] Log injection / size limits (large chunks rejected, dangerous content escaped)
<!-- SECTION:DESCRIPTION:END -->

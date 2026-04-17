---
id: TASK-170
title: 'Epic: Performance & Load Tests'
status: Backlog
assignee: []
created_date: '2026-03-04 03:09'
labels: []
milestone: m-15
dependencies: []
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem
Performance and load tests needed to prove the system works under realistic volume.

## Goals
- Claim throughput (many builders polling)
- Log streaming volume (sustained uploads)
- Cache push backlog (thousands of jobs)
- DPM scalability (many systems + derivations)

## Scope
Load testing, performance benchmarking, scalability verification.

## Acceptance Criteria
- [ ] Claim throughput: many concurrent builders, p95 latency within target, no double-claims
- [ ] Log streaming volume: DB growth acceptable, API doesn't OOM
- [ ] Cache push backlog: worker fairness, no starvation, retry_after respected
- [ ] DPM scalability: periodic convergence completes within interval without locking contention
<!-- SECTION:DESCRIPTION:END -->

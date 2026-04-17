---
id: TASK-171
title: 'Epic: Property-Based / Generative Tests'
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
Property-based and generative tests needed to catch race conditions and edge cases with randomized event sequences.

## Goals
- Randomized event sequences (claim, renew, log, complete, fail, crash, lease-expire, retry)
- Commutativity of retries (reorder duplicate requests)
- Verify invariants: no double terminal states, no duplicate cache push, gc_root lifecycle correctness

## Scope
Generative testing with proptest or similar, random event scheduling.

## Acceptance Criteria
- [ ] Randomized event sequences maintain invariants:
  - No job ends in two terminal states
  - No duplicate cache push job
  - gc_root exists iff (build success AND cache not completed)
  - "deployable" implies required statuses
- [ ] Commutativity of retries: final state equals expected safe outcome
<!-- SECTION:DESCRIPTION:END -->

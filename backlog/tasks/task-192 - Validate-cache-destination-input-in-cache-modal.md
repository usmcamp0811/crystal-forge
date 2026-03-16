---
id: TASK-192
title: Validate cache destination input in cache modal
status: To Do
assignee: []
created_date: '2026-03-14 16:49'
updated_date: '2026-03-16 23:17'
labels:
  - frontend
  - validation
  - cache
  - ux
dependencies: []
references:
  - packages/web-ui/src/views/caches.rs
priority: medium
ordinal: 5000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem
The cache destination modal currently accepts invalid/garbage cache endpoint input, which can lead to misconfigured destinations and confusing failures later.

## Desired Outcome
Add robust validation in the cache destination modal so users can only submit syntactically valid cache destination values (and receive clear inline error messages when input is invalid). Include validation behavior for relevant cache types and malformed input cases.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Cache destination modal rejects malformed destination input before submit.
- [ ] #2 Validation messages are shown inline and indicate what is wrong with the value.
- [ ] #3 Valid destination inputs for supported cache types are accepted.
- [ ] #4 Existing cache destination create flow remains functional for valid inputs.
<!-- AC:END -->

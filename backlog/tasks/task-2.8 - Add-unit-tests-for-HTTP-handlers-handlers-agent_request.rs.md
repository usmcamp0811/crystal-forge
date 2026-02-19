---
id: TASK-2.8
title: Add unit tests for HTTP handlers - handlers/agent_request.rs
status: Review
assignee: []
created_date: '2026-02-04 20:39'
updated_date: '2026-02-19 03:53'
labels:
  - testing
  - handlers
  - http
milestone: m-1
dependencies: []
parent_task_id: TASK-2
---

## Notes

LOCK: claude on gray in ~/code/crystal-forge/TASK-2.8-agent-request-tests

## Implementation Summary

Added comprehensive unit tests for `handlers/agent_request.rs`:

1. Created `SystemLookup` trait to abstract database queries, enabling mock testing
2. Added `authenticate_agent_request_with_lookup` function that accepts any `SystemLookup` implementation
3. Implemented `MockSystemLookup` for testing without a real database
4. Added 14 unit tests covering:
   - Valid signature authentication
   - Invalid signature rejection
   - Missing headers (X-Key-ID, X-Signature, both)
   - Unknown hostname handling
   - Database error handling
   - Invalid base64 signature format
   - Wrong signature length
   - Tampered body detection
   - SystemState deserialization (current and V1 versions)

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Test authentication logic in isolation using mock requests and keys.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Test authenticate_agent_request with valid signature
- [x] #2 Test with invalid signature
- [x] #3 Test with missing headers
- [x] #4 Test with unknown hostname
- [ ] #5 Use axum-test for handler testing (deferred - unit tests use mock approach)
- [x] #6 Mock database responses
<!-- AC:END -->

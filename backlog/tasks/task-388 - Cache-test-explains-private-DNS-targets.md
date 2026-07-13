---
id: TASK-388
title: Cache test explains private DNS targets
status: To Do
assignee: []
created_date: '2026-07-09 00:00'
updated_date: '2026-07-09 00:00'
labels:
  - caches
  - backend
  - ux
priority: high
ordinal: 322000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

The cache destination **Test** button rejects split-horizon/LAN DNS targets with a generic private-IP SSRF error. This is confusing for operators whose public HTTPS cache host intentionally resolves to a private proxy address on the LAN.

## Goal

Keep SSRF protections enabled by default, but make private-target failures actionable by explaining the policy and the explicit `server.allow_private_cache_test_targets` opt-in.

## Non-Goals

- Do not disable the default SSRF guard.
- Do not automatically allow private DNS targets.
- Do not expose cache credentials, bearer tokens, or other secrets in UI or logs.

## Architectural Constraints

- Preserve the existing server-side URL and DNS validation boundary.
- Error messaging must be produced server-side so API and UI clients receive consistent guidance.
- UI should display the server-provided error without adding cache-specific security policy logic to view components.

## Acceptance Criteria

- [ ] Cache test failures caused by private/loopback/non-routable DNS resolution mention that private targets are blocked by default.
- [ ] The failure message points operators to `server.allow_private_cache_test_targets = true` / `CRYSTAL_FORGE__SERVER__ALLOW_PRIVATE_CACHE_TEST_TARGETS=true` for intentional LAN or split-horizon DNS setups.
- [ ] Default SSRF behavior remains blocked unless the explicit setting is enabled.
- [ ] No response, log, or UI message exposes cache credentials or bearer tokens.

## Verification Plan

- Run targeted backend tests for cache destination test validation.
- Add/update unit tests for the private resolved-address error message.
- Run formatter and targeted backend check.

## Impact Areas

- `packages/default/src/handlers/api/caches.rs`
- cache destination dialog error display, if needed

## Risk Level

Low. Message-only UX improvement around an existing security guard and explicit opt-in.

## Dependencies

- None.
<!-- SECTION:DESCRIPTION:END -->

## Notes

<!-- SECTION:NOTES:BEGIN -->
- Selected for next work by maintainer on 2026-07-09.
<!-- SECTION:NOTES:END -->

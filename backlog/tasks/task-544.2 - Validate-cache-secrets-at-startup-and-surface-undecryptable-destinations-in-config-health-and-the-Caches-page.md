---
id: TASK-544.2
title: >-
  Validate cache secrets at startup and surface undecryptable destinations in
  config-health and the Caches page
status: To Do
assignee: []
created_date: '2026-11-24T23:27:00Z'
labels:
  - cache
  - security
dependencies:
  - TASK-544.1
parent_task_id: TASK-544
priority: high
type: enhancement
ordinal: 581000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem
TASK-544 slice B. Nothing checks that the configured CRYSTAL_FORGE_CACHE_ENCRYPTION_KEY decrypts the stored secrets. A wrong or wrongly rendered key file shows up only through failing dispatch and CVE scans.

## Goal
The server detects an undecryptable cache secret at startup and when a destination changes, and shows it as a configuration fault on GET /api/v1/admin/config-health and on the Caches page.

## Non-goals
Key rotation or re-encryption tooling; dispatch behavior (544.1); builder/queue UI (544.3); CVE worker (544.4).

## Architectural constraints
Reuse the typed decrypt error from 544.1. The check decrypts and discards; it never returns, logs or stores a value. Result is derived on demand or at startup; no new secret storage. Follow the existing config_health query/response pattern. Caches page change follows the existing Caches view and design.

## Verification plan
Nix farm builds only. Unit test with wrong key; config-health handler test; owning integration-delivery scenario: store a secret with key A, start with key B, assert the fault, then correct key clears it. Browser-visible, so live preview and MR screenshot are required. nix build .#okf-knowledge -L.

## Impact areas
cf-server queries/config_health.rs, handlers/api/config_health.rs, server startup, cache destination handlers; web-ui Caches view; checks/integration-delivery; docs/knowledge.

## Risk
Low to medium. Read-only check; UI is additive.

## Dependencies
TASK-544.1.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 At server startup and after a cache destination is created or updated, the server tries to decrypt every enc:v1 value in cache_destinations. On failure it logs one ERROR naming destination id, name and column (never the value).
- [ ] #2 GET /api/v1/admin/config-health reports each undecryptable destination (id, name, column) as a fault, and reports none when all decrypt.
- [ ] #3 The Caches page shows the fault on the affected destination.
- [ ] #4 Integration-delivery scenario: a secret encrypted with key A and a server started with key B shows the config-health fault; the correct key clears it. No secret appears in logs or responses.
<!-- AC:END -->

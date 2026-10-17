---
id: TASK-490.13
title: >-
  Delivery: Attic, S3, and HTTP/Nix cache adapter variants (sequential within
  publication)
status: Backlog
assignee: []
created_date: '2026-10-17 16:37'
labels:
  - testing
  - integration
  - luna
dependencies:
  - TASK-490.12
parent_task_id: TASK-490
priority: medium
type: task
ordinal: 525000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Owning check: integration-delivery. Contract ID: delivery.publication.variants. Exclusive write path: tests/integration/delivery/scenarios/publication-variants.{nix,py,cjs}. Implement the variants sequentially in this order: Attic, S3, HTTP/Nix. Each variant proves only its distinct boundary; the common evaluate-build-retrieve workflow is proven once by TASK-490.12 and is reused, not cloned.

WHY: legacy attic-metrics, attic-publication, and web-ui steps 21-25 prove adapters partly with SQL clones and a private fixture table. Each supported CacheType must prove its own publication or read authentication once.

SCOPE: Attic JWT write with scope negatives and named-cache Test; S3 access-key write and read; HTTP and Nix read-only destinations (public read, no write path). Each variant: one fresh output, assert_store_absent at the destination first, wrong credentials publish nothing. Non-goals: metrics accounting internals (non-integration).

READY WHEN: TASK-490.12 is integrated and TASK-490.7 provides Attic and S3 fixtures. Source to read then: cf-protocol src/cache.rs, cf-server handlers/api/caches.rs (test-credentials), derivations/cache.rs, cf-builder Attic/S3 push; docs/knowledge/caches/*; checks/attic-metrics/README.md, checks/attic-publication/README.md.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Attic, S3, and HTTP/Nix each prove their distinct publication or read authentication boundary with a fresh output
- [ ] #2 Wrong credentials publish nothing for each write-capable adapter
- [ ] #3 No complete lifecycle is duplicated per adapter
- [ ] #4 Executed in real VMs with recorded evidence
<!-- AC:END -->

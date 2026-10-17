---
id: TASK-490.12
title: 'Delivery: Niks3 publication, authentication, and cold retrieval scenario'
status: Backlog
assignee: []
created_date: '2026-10-17 16:37'
labels:
  - testing
  - integration
  - luna
dependencies:
  - TASK-490.10
parent_task_id: TASK-490
priority: high
type: task
ordinal: 524000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Owning check: integration-delivery. Contract ID: delivery.publication.niks3. Exclusive write paths: tests/integration/delivery/scenarios/publication-niks3.{nix,py,cjs}.

WHY: Niks3 is the newest first-class cache (TASK-470). Legacy niks3-cache/niks3-write-probe/niks3-ui prove pieces with seeded queue rows. This scenario replaces them with one real chain: evaluation -> build -> publication -> verified retrieval.

SCOPE: cache configured through the Caches UI (form setup only; form maintenance is Access) with token write auth, then mTLS write auth; a fresh output built on builderA is published to the destination bound to its ID; the server verifies publication by an independent read; targetA (cold store, assert_store_absent first) retrieves it with signature verification; wrong token/client certificate is rejected and publishes nothing; agents in another environment receive no read credential. Non-goals: Attic/S3/HTTP/Nix (490.13); activation (490.14).

READY WHEN: TASK-490.10 is integrated and TASK-490.7 provides the Niks3 cache fixture on the services role. Source to read then: cf-protocol src/cache.rs (CacheType, CacheReadAuth, Niks3WriteAuth), cf-server handlers/api/caches.rs, builders.rs (publication, completion), derivations/cache.rs, cf-builder Niks3 push, cf-agent pull path; docs/knowledge/caches/niks3-cache.md; checks/niks3-cache/README.md (legacy assertions to carry forward where authentic); lib/cache-test-node.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A fresh output is published to Niks3 under token and mTLS write auth and verified by an independent read
- [ ] #2 A cold target retrieves it with signature verification
- [ ] #3 Wrong credentials publish nothing; out-of-environment agents get no read credential
- [ ] #4 Executed in real VMs with recorded evidence; legacy Niks3 checks are mapped as superseded
<!-- AC:END -->

---
id: TASK-490.10
title: 'Delivery: builder execution strategies and transport authorization scenario'
status: Backlog
assignee: []
created_date: '2026-10-17 16:37'
labels:
  - testing
  - integration
  - luna
dependencies:
  - TASK-490.7
parent_task_id: TASK-490
priority: high
type: task
ordinal: 522000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Owning check: integration-delivery. Contract ID: delivery.build.strategies. Exclusive write paths: tests/integration/delivery/scenarios/build-strategies.{nix,py,cjs}.

WHY: both supported RemoteBuildExecutionStrategy values (ServerDerivation, SourceReEvaluateVerified; source delivery ServerBundledArchive only) must build a real fresh output on a real builder through the authorized builder API. Legacy proof seeds queue rows (niks3-cache).

SCOPE: builderA uses ServerDerivation and builderB uses SourceReEvaluateVerified; each builds a per-run fresh output that assert_store_absent proved missing beforehand; the server records completion only for the authorized builder session; a builder with a wrong key or a revoked session is rejected and no job state changes; derivation/source archive downloads require the job authorization. Builds view shows the exact job, builder, and output. Non-goals: scheduling/cancel (490.11); cache publication (490.12).

READY WHEN: TASK-490.7 is integrated. Source to read then: cf-protocol src/builder.rs (RemoteBuildExecutionStrategy, SourceInputDeliveryMode, NextJobRequest), cf-server handlers/api/builders.rs (session, next-job, jobs/:jid/*), cf-builder crate; docs/knowledge/builders/verified-source-evaluator-contract.md, docs/knowledge/api/builder-job-lifecycle-api.md, docs/knowledge/security/builder-request-authentication-and-data-in-transit.md; design BuildsView.jsx, BuildersView.jsx.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Each supported strategy builds a fresh output on its own real builder
- [ ] #2 Unauthorized or revoked builder sessions are rejected with no job state change
- [ ] #3 Archive downloads enforce job authorization
- [ ] #4 Builds view shows the exact job, builder, and output identity
- [ ] #5 Executed in real VMs with recorded evidence
<!-- AC:END -->

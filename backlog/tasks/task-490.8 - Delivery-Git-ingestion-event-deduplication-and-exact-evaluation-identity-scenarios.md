---
id: TASK-490.8
title: >-
  Delivery: Git ingestion, event deduplication, and exact evaluation identity
  scenarios
status: Backlog
assignee: []
created_date: '2026-10-17 16:36'
labels:
  - testing
  - integration
  - luna
dependencies:
  - TASK-490.7
parent_task_id: TASK-490
priority: high
type: task
ordinal: 520000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Owning check: integration-delivery. Contract IDs: delivery.evaluation.ingestion, delivery.evaluation.identity. Exclusive write paths: tests/integration/delivery/scenarios/evaluation-ingestion.{nix,py,cjs}, tests/integration/delivery/scenarios/evaluation-identity.{nix,py,cjs}, tests/integration/delivery/fixtures/evaluation-*.

WHY: legacy checks/integration seeds commits and derivations with SQL; web-ui 13h is the only real force-push proof. Exact identity (commit -> drvPath -> Evaluations view record) has no integration proof.

SCOPE: ingestion — polling and webhook for two authored commits create exactly one commit row each; replayed webhooks are deduplicated; force-push rewrite is detected and surfaced; the Flakes view shows the rewrite state. Identity — two distinguishable commits evaluate to the drvPath that an independent `nix eval` of the same commit computes on the services role; the Evaluations list and /evaluations/:commit_id show the exact record; the EvalDrawer opens the same identity; moving the branch head does not change a historical record. Extend (do not duplicate) the coordinator's proving scenario from TASK-490.7. Non-goals: Config Inspector (490.9); builds (490.10).

READY WHEN: TASK-490.7 is integrated. Source to read then: cf-server handlers/api/flakes.rs, commits.rs, webhook.rs, evaluation coordinator modules; web-ui views flakes*.rs, evaluations*.rs, components EvalDrawer; design FlakesView.jsx, EvalsView.jsx, EvalDrawer.jsx; docs/knowledge/evaluation/*, docs/knowledge/api/flakes-and-evaluation-snapshot-api.md.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Each authored commit is ingested exactly once through polling and webhook; replays are deduplicated; force-push rewrite is surfaced
- [ ] #2 Recorded drvPaths equal independently computed nix eval results for two distinguishable commits
- [ ] #3 Evaluations list, detail route, and drawer show the exact identity; historical records stay exact after the head moves
- [ ] #4 Executed in real VMs with recorded evidence
<!-- AC:END -->

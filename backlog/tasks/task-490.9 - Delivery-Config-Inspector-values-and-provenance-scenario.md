---
id: TASK-490.9
title: 'Delivery: Config Inspector values and provenance scenario'
status: Backlog
assignee: []
created_date: '2026-10-17 16:36'
labels:
  - testing
  - integration
  - luna
dependencies:
  - TASK-490.8
parent_task_id: TASK-490
priority: medium
type: task
ordinal: 521000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Owning check: integration-delivery. Contract ID: delivery.evaluation.config-inspector. Exclusive write paths: tests/integration/delivery/scenarios/config-inspector.{nix,py,cjs}, tests/integration/delivery/fixtures/config-inspector-*.

WHY: Config Inspector views are proven by mocked steps (12l-12q, 13j-13q); the Nix-level checks (checks/config-inspector, config-observer) do not exercise the server worker, API, or UI. Separate from general evaluation because it needs its own worker, provenance fixture, and drawer bindings.

SCOPE: for an authored fixture flake with deliberately distinct option values and definition files, the System Config tab and the flake Config Inspector show the exact values and definition provenance for the exact revision; a secret-path option is redacted; a second revision with a changed value is distinguishable; partial-inventory state appears only when the worker reports it. Non-goals: Nix expression unit behavior (stays in non-integration checks).

READY WHEN: TASK-490.8 is integrated. Source to read then: cf-server bin/config-inspector-worker.rs, handlers/api/systems.rs (config-inspections, config-observations, evaluated-options), flakes.rs (revisions/:rev/outputs, modules/:module/declarations); web-ui system detail Config tab and flake explorer; design ConfigExplorer.jsx, FlakeExplorer.jsx; docs/knowledge/evaluation and config inspector concepts; checks/config-inspector/fixture (adapt, do not modify).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Inspector values and provenance match authored fixture facts for the exact revision
- [ ] #2 Secret-path values are redacted in API and UI
- [ ] #3 A changed second revision is distinguishable and historical selection stays exact
- [ ] #4 Executed in real VMs with recorded evidence
<!-- AC:END -->

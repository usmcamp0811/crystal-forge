---
id: TASK-440.2
title: Harden Config observation scheduling and large-index behavior
status: Backlog
assignee: []
created_date: '2026-09-18 03:51'
labels:
  - config
  - performance
  - follow-up
  - TASK-440
dependencies: []
references:
  - TASK-440
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/323'
documentation:
  - docs/config-explorer-architecture.md
modified_files:
  - packages/default/crates/cf-server/src/services/config_observations.rs
  - packages/default/crates/cf-server/src/queries/config_observations.rs
  - packages/web-ui/src/components/system/config_explorer.rs
  - checks/config-observer/default.nix
parent_task_id: TASK-440
priority: high
type: enhancement
ordinal: 475000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
TASK-440 established revision-scoped shallow Config observations, automatic visible-value previews, and a bounded Configured index. Runtime evidence shows that large Configured classification can approach 1 GiB and still depends on conservative heavy-Nix serialization. The follow-up must make observation latency and resource use predictable without weakening immutable-source identity, explicit-request priority, redaction, or side-effect-free read behavior.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Large Configured-index observations complete within documented memory and latency budgets on the representative repository fixture.
- [ ] #2 Explicit Config inspections are never queued behind pending automatic previews or background Configured-index work.
- [ ] #3 Automatic visible-option previews remain bounded and work consistently in Browse and Configured modes without eager prefix expansion or provenance requests.
- [ ] #4 Observation cancellation, retry, ownership, and stale-result fencing remain correct across revision and system changes.
- [ ] #5 Immutable source identity, pre-persistence redaction, side-effect-free reads, and supported evaluator compatibility remain unchanged.
- [ ] #6 Focused real-Nix, scheduler, persistence, frontend, and browser tests cover the resulting resource and interaction contracts.
- [ ] #7 Config observation architecture documentation records the final capacity model, budgets, priorities, and failure behavior.
<!-- AC:END -->

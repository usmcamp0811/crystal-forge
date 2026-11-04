---
id: TASK-517
title: >-
  Explain queued builds that no builder is authorized to claim, and fix
  queued_jobs wildcard count
status: Backlog
assignee: []
created_date: '2026-11-04T02:11:00Z'
labels: []
dependencies: []
references:
  - packages/default/crates/cf-server/src/queries/builders.rs
  - packages/web-ui/src/views/builds.rs
priority: medium
type: bug
ordinal: 568000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Found in the TASK-508 review. (1) A build job can stay queued while no builder is authorized to claim it (environment assignment mismatch). The Builds page shows no reason. Show why a queued job is unclaimable. (2) Pre-existing: the builder summary queued_jobs query in queries/builders.rs (around lines 317-326) does not treat a builder with no environment assignment as a wildcard, but the claim logic does. The Builders page can undercount eligible work. Both are outside TASK-508 scope.
<!-- SECTION:DESCRIPTION:END -->

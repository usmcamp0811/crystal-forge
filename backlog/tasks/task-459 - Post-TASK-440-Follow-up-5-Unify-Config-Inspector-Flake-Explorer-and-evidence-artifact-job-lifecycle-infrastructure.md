---
id: TASK-459
title: >-
  Post-TASK-440 Follow-up 5: Unify Config Inspector, Flake Explorer, and
  evidence-artifact job/lifecycle infrastructure
status: To Do
assignee: []
created_date: '2026-09-06 23:08'
labels: []
dependencies:
  - TASK-440
  - TASK-456
documentation:
  - >-
    backlog/docs/doc-24 -
    Crystal-Forge-Evaluation-Evidence-Build-Admission-and-Deployment-Gating-Architecture.md
priority: medium
type: enhancement
ordinal: 468000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

The TASK-440 Config Inspector and Flake Explorer independently implement overlapping execution/lifecycle concerns: revision identity handling, immutable artifact lifecycle, content addressing, redaction, current-selector logic, retention, DB-only bounded readers, job orchestration, cancellation/resource limits, and audit identity. This is "Follow-up Task 5 — Unify artifact/job infrastructure" from the "Crystal Forge Evaluation, Evidence, Build Admission, and Deployment Gating Architecture" design document (doc-24, §15, §21).

This task depends on the evidence identity/version contract defined by the sibling Follow-up Task 1 (TASK-456) so the shared infrastructure is built against one agreed artifact-identity model rather than a new ad hoc one.

## Non-goals

- Do not collapse Flake Explorer's semantic data model into the Config artifact model, or vice versa; per design §15, `ConfigArtifact` and `FlakeOutputArtifact` remain semantically separate artifact types answering different questions.
- Do not merge per-configuration inspection so that inspecting one configuration requires evaluating sibling configurations or unrelated exported modules; per design §16, per-configuration failure isolation is mandatory and must be preserved through this refactor.
- Do not change the external API/DTO contract for existing Config Inspector or Flake Explorer consumers unless a specific incompatibility is identified and explicitly called out and approved.
- Do not implement CVE artifact or build/closure artifact producers in this task; only refactor shared infrastructure for existing producers (Config Inspector, Flake Explorer).

## Scope

1. Identify the concrete shared infrastructure surface between Config Inspector and Flake Explorer: revision/exact identity handling, immutable artifact lifecycle transitions, content addressing, redaction boundary, certification, current-selector resolution, retention/GC interaction, bounded DB-only readers, job/execution orchestration, cancellation/resource limits, and audit identity.
2. Extract or consolidate this shared surface into common execution/lifecycle infrastructure where doing so does not require merging the two artifact types' semantic data models, per design §15.
3. Preserve per-configuration and per-target failure isolation: a broken or poisoned configuration or flake target must not contaminate another target's inspection or a sibling's fast evaluation, per design §16.
4. Preserve existing redaction guarantees (secret-bearing values redacted before persistence/indexing/diffing/logging/API exposure) across the unified infrastructure.
5. Preserve existing retention, GC-interaction, and immutable-artifact-lifecycle correctness already established by TASK-440 for `ConfigArtifactV2`; extend the same guarantees to Flake Explorer's artifact lifecycle where it is folded into shared infrastructure.

## Verification plan

- `nix develop -c cargo check --package cf-server` and targeted `cargo test` coverage proving Config Inspector and Flake Explorer both continue to produce their existing artifact shapes and existing API behavior after the infrastructure refactor.
- Regression tests proving per-configuration/per-target failure isolation is preserved (a broken sibling configuration/module does not contaminate another target's inspection).
- Regression tests proving redaction, retention, and immutable-lifecycle guarantees established for `ConfigArtifactV2` in TASK-440 are preserved after the refactor.
- Relevant Nix checks (e.g. `nix build .#checks.x86_64-linux.config-inspector`, and the equivalent Flake Explorer check if one exists) pass.
- `git diff` review confirming no unintended API/DTO contract change; SQLx offline metadata updated only if the refactor changes query/result shapes.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A documented inventory identifies the concrete shared infrastructure surface between Config Inspector and Flake Explorer (identity, lifecycle, content addressing, redaction, selectors, retention, bounded readers, job orchestration, cancellation/resource limits, audit identity).
- [ ] #2 Shared execution/lifecycle infrastructure is extracted or consolidated for Config Inspector and Flake Explorer without merging their semantic data models; ConfigArtifact and FlakeOutputArtifact remain distinct types.
- [ ] #3 Per-configuration and per-target failure isolation is preserved: a regression test proves a broken/poisoned configuration or flake module does not contaminate another target's inspection or a sibling's fast evaluation.
- [ ] #4 Existing redaction guarantees (secret-bearing values redacted before persistence, indexing, diffing, logging, and API exposure) are preserved across the unified infrastructure for both Config Inspector and Flake Explorer.
- [ ] #5 Existing immutable-artifact retention and lifecycle correctness established for ConfigArtifactV2 by TASK-440 is preserved, and equivalent guarantees are established for Flake Explorer artifacts where they now share infrastructure.
- [ ] #6 No unintended external API/DTO contract change occurs for existing Config Inspector or Flake Explorer consumers; any identified necessary change is explicitly documented and approved.
- [ ] #7 Targeted server tests and relevant Nix checks (including the config-inspector check) pass after the refactor.
<!-- AC:END -->

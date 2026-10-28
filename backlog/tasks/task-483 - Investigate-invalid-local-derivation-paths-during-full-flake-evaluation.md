---
id: TASK-483
title: Investigate invalid local derivation paths during full flake evaluation
status: Backlog
assignee: []
created_date: '2026-10-12 14:27'
labels: []
dependencies: []
references:
  - packages/dev-env/default.nix
  - checks/niks3-cache/default.nix
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/331'
priority: medium
type: spike
ordinal: 505000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
TASK-470 Attic metrics targeted native/browser checks pass, but `nix flake check --keep-going --no-build` fails locally while evaluating packages.x86_64-linux.dev-env (missing agent.pub.drv) and checks.x86_64-linux.niks3-cache (missing niks3-mtls-split.drv). A command-local `--option eval-cache false` retry reproduces the same missing paths. The new attic-metrics package/module/check evaluate successfully. Earlier browser artifacts disappeared locally; retained logs establish a larger subsequent rebuild/refetch plan, but do not establish GC or disk pressure as the cause. Diagnose local store/derivation context and reproduce on a clean CI runner before proposing source or global Nix changes. Do not weaken signatures, tests, cache policy, or touch production data.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Identify a reproducible cause for invalid derivation paths without inventing GC or disk-pressure evidence
- [ ] #2 Determine whether the failure reproduces on a clean exact-head CI runner
- [ ] #3 Document a bounded recovery or separately approved source correction that preserves cache signatures and fixture semantics
<!-- AC:END -->

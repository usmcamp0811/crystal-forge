---
id: TASK-490.1
title: >-
  Integration overhaul foundation: contract, coverage map, shared harness, and
  Access bootstrap slice
status: In Progress
assignee:
  - claude-opus-5-5
created_date: '2026-10-17 16:13'
updated_date: '2026-10-17 16:14'
labels:
  - testing
  - integration
dependencies: []
references:
  - /home/mcamp/code/crystal-forge/crystal-forge-integration-test-contract.md
  - packages/default/crates/cf-server/src/bin/server.rs
  - packages/default/crates/cf-server/src/handlers/api/auth_local.rs
  - packages/default/crates/cf-server/src/auth/session.rs
  - modules/nixos/crystal-forge/default.nix
  - packages/web-ui/src/routes.rs
  - checks/web-ui/default.nix
parent_task_id: TASK-490
priority: high
type: task
ordinal: 513000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Problem: the four-owner integration architecture (parent TASK-490) has no shared harness, no adopted contract in the repository, and no coverage or retirement map. Children cannot start without a tested shared interface and exact registration mechanism.

Goal: adopt the contract, record coverage and retirement maps, build the smallest shared NixOS-test harness the first scenario and the bounded children need, and prove it with one real executed Access vertical slice: fresh server/database startup with normal module configuration, supported local first-admin bootstrap, real production-UI browser sign-in, and an authoritative reload/read.

Non-goals: no final flake check is exported (owners stay unexported until complete); no legacy check or CI job is removed; no product or module fix; no generic DSL, coverage database, generated registry, or new CI service; no edits to docs/design/CrystalForge/.

Architectural constraints: test-only scope from TASK-490. Harness lives in tests/integration/ (not a snowfall-discovered directory). Max five VM roles (controller, services, builderA, builderB, targetA); only needed roles instantiated. Checks share immutable inputs only. Production server package is cf-server-drv (embedded UI) selected through the normal module.

Dependencies: none. Base origin/dev 33de73b013b735304dfac9c69745a074a3e76ebd.
Risk: medium (shared interface that all children depend on; VM runtime cost).
Impact areas: tests/integration/**, docs/knowledge/testing/**, AGENTS.md, docs/agents/verification.md, backlog records.
Live preview: not applicable; this task changes no browser-visible behavior (test-only).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Contract is committed at docs/knowledge/testing/integration-test-contract.md with implementation_status partial and an adoption note that distinguishes adopted rules from implemented checks; AGENTS.md and docs/agents/verification.md link to it
- [ ] #2 docs/knowledge/testing/integration-coverage-map.md contains the obligation map (contract ID to owner, scenario, independent expected result, evidence, status) and the legacy retirement map (every checks/* directory and CI job with a disposition)
- [ ] #3 tests/integration/ provides a role/topology interface, production package selection, deterministic facts plus runtime receipts, bounded waits, fault-injection helper, pinned browser/render profile, failure artifacts, and one coordinator-owned scenario registration point per owner
- [ ] #4 A focused owner-local command runs a named scenario in a real NixOS VM test with baseline updating disabled and propagates failure
- [ ] #5 The Access bootstrap slice executes in a real VM (not only evaluated or built) and its actual outcome is recorded with command and evidence; a product defect, if exposed, keeps its failing assertion and gets a linked Backlog defect
- [ ] #6 nix flake show / eval confirms no new flake check attribute was created by tests/integration and no existing check changed
- [ ] #7 Bounded Luna child tasks exist with concrete commits, paths, helper signatures, fixtures, scenarios, commands, and completion criteria; dependency-ready ones are To Do
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
LOCK: claude-opus-5-5 on sledge in /home/mcamp/code/crystal-forge/TASK-490.1-integration-foundation

Preflight 2026-10-17: branch TASK-490.1-integration-foundation from origin/dev 33de73b013b735304dfac9c69745a074a3e76ebd (owner chose origin/dev over local dev 91809dd8, which has 10 unpushed TASK-488 backlog commits). dev worktree has only pre-existing untracked files (backlog/.locks/TASK-478.lock, session-ses_f927.md), preserved. No main worktree exists. Host sledge: 32 cores, 123 GiB RAM (~45 GiB available), /dev/kvm present. Live preview: not applicable (test-only; no browser-visible behavior change).
<!-- SECTION:NOTES:END -->

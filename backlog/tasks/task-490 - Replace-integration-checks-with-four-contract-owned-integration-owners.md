---
id: TASK-490
title: Replace integration checks with four contract-owned integration owners
status: In Progress
assignee:
  - claude-opus-5-5
created_date: '2026-10-17 16:13'
updated_date: '2026-10-17 20:35'
labels:
  - testing
  - integration
dependencies: []
references:
  - /home/mcamp/code/crystal-forge/crystal-forge-integration-test-contract.md
  - AGENTS.md
  - docs/agents/verification.md
  - docs/knowledge/testing/flake-checks.md
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/335'
priority: high
type: task
ordinal: 512000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The current integration layer mixes about 14 VM checks and a 26.9k-line browser suite. Many browser steps use route mocks; OIDC completion, real scanner evidence, real activation, and a binary old-version upgrade are incomplete. This parent tracks the owner-approved test-only overhaul into exactly four native Snowfall owners: integration-access, integration-delivery, integration-assurance, integration-upgrade.

Contract source: /home/mcamp/code/crystal-forge/crystal-forge-integration-test-contract.md, adopted at docs/knowledge/testing/integration-test-contract.md. The corrected organization is native Snowfall: checks/integration-{access,delivery,assurance,upgrade}/default.nix with {lib,inputs,pkgs,...}; child scenarios and assets are owner-local; shared helpers are lib/integration-test via lib.crystal-forge.makeIntegrationTestHarness {pkgs,inputs}. Explicit coordinator registration and normal pkgs.linkFarm aggregation are required. No root tests/, legacyPackages, custom discovery or placeholder owners. Non-integration checks and legacy CI gates remain.

Test-only boundary: no production functionality changes, Rust implementation changes, migrations, SQLx metadata, module semantics, UI assets, or docs/design/CrystalForge edits. Production defects remain linked Backlog tasks with executable assertions; they are not fixed by this parent.

Current foundation: native Access scenario has executed on combined source commit a6c8922650bf7f0ff643d181422c0092cfc43242 (TASK-491 prerequisite cherry-picked). The run fails before migrations/HTTP: generated database host `/run/postgresql` becomes an empty-authority PostgreSQL URI; generated config also omits auth_mode and the service has AUTH_MODE unset. Evidence and exact command are recorded in docs/knowledge/testing/integration-coverage-map.md. Product defects TASK-490.25/.26 are Backlog and not implemented. The Access assertion remains. TASK-491 package repair is in MR !336; normal package check passes, but this does not make the Access scenario pass.

Packet corrections: all scenario runs have fresh VM/database state; dependencies provide code or fixture definitions, never runtime rows, sessions, IDs, or outputs. Assurance triage (.20), plan creation (.21), finding waiver (.27), acceptance register (.28), and POA&M verification/closure (.29) have distinct task boundaries. All child packets remain Backlog; none is dispatch-ready until the product configuration blockers, a successful tested foundation dependency, accepted authority, implemented helpers/roles, explicit registration and focused commands are verified.

Coordinator-owned paths: owner default.nix registration, lib/integration-test, flake/CI, canonical contract/map and directly affected agent/testing docs. Child writers use exclusive scenario and fixture paths. TASK-490.7 and .24 are coordinator tasks. Overhaul branch: TASK-490.1-integration-foundation, based on origin/dev 33de73b013b735304dfac9c69745a074a3e76ebd; current combined work is in the dedicated TASK-490.1 branch. No merge/deploy is authorized. Parent remains In Progress; final cutover and Done are blocked until all required owners pass or the owner explicitly changes scope.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The contract is adopted at docs/knowledge/testing/integration-test-contract.md with accurate implementation_status and directly affected AGENTS.md/verification guidance; as-built D1 is not accepted authority and uncertain cases require an owner decision
- [ ] #2 The coverage map assigns every accepted obligation one primary owner, scenario, independent expected result, evidence and status, and records a retirement disposition for every legacy integration check and CI job
- [ ] #3 At final cutover exactly four native Snowfall integration owners exist under checks/integration-{access,delivery,assurance,upgrade}/default.nix using { lib, inputs, pkgs, ... }; non-integration checks retain their own attributes and classification
- [ ] #4 Each normal owner is a pkgs.linkFarm aggregate of ALL explicitly registered scenario gates and propagates every failure; meaningful WIP checks may be discoverable, and no placeholder owner is added
- [ ] #5 Shared helpers use lib/integration-test through lib.crystal-forge.makeIntegrationTestHarness { pkgs, inputs }; scripts/fixtures/assets are owner-local, coordinator registration is explicit, and there is no root tests/ tree, legacyPackages.integration, mkOwner, or custom discovery
- [ ] #6 Legacy registrations and CI jobs are retired only after passing replacements or an explicit owner-approved scope change; correct product-defect assertions remain executable and linked to Backlog bugs
- [ ] #7 No production code, migrations, SQLx metadata, UI assets, or design-handoff files change; child packets remain Backlog until actual dependency commits and verified commands exist
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Organization correction recorded 2026-10-17: this task's description and AC supersede prior layout and incomplete-unexported rules. Backlog grooming only; no implementation, Luna delegation, or slice execution is claimed. Foundation dependency commit is unresolved. Foundation is blocked by two existing Rust package tests; no VM execution. All 24 children remain Backlog, none dispatch-ready. Future packets need actual dependency commit(s) and verified commands before To Do. TASK-490.7/.24 are coordinator writer tasks; TASK-490.20/.21 need finer splitting of triage, policy waiver, and closure without scope expansion now.

Historical coordination context: prior owner claude-opus-5-5 recorded the foundation worktree /home/mcamp/code/crystal-forge/TASK-490.1-integration-foundation. That historical record is not evidence of a completed foundation or current dispatch readiness.

LOCK: claude-opus-5-5 on sledge in /home/mcamp/code/crystal-forge/TASK-490.1-integration-foundation

Run one VM test at a time until measured capacity supports a higher limit. Foundation remains In Progress, blocked pending package build; dependent children remain Backlog. No foundation readiness or verification is claimed.

Corrected native-layout WIP preserved and pushed: 683c86e03ef14b37e69c00030d6d17660398324d; draft MR !335 (not merge-ready). Only integration-access is discoverable (27 total checks, previous 26 unchanged). No root tests tree/custom discovery/flake-output assembly remains. TASK-491 blocks VM execution; foundation incomplete and Luna children stay Backlog. No Luna dispatch, deployment, merge, Review, or Done transition occurred.

Foundation resumed on 2026-10-17 in the existing dedicated worktree. Integrated tested TASK-491 dependency by cherry-pick `af39284bdf16d56402f01a64c916f17948e43b02` as `a6c8922650bf7f0ff643d181422c0092cfc43242`. The exact Access scenario executed in a VM and failed at startup, preserving its assertion. Generated config evidence reveals product blockers TASK-490.25 (local PostgreSQL socket host becomes empty-authority URI) and TASK-490.26 (local auth_mode omitted; AUTH_MODE unset; route behavior not reached because DB URL failure comes first). Both are Backlog, not fixed. Coverage map now records exact evidence and marks the scenario failing, not passing. Assurance packets .20/.21 were narrowed; .27/.28/.29 split waiver, acceptance register, and verify/closure. Per-scenario fresh DB/VM rule now says task deps never share runtime outputs. All child packets remain Backlog; no Luna dispatch, merge, deploy, or application fix.
<!-- SECTION:NOTES:END -->

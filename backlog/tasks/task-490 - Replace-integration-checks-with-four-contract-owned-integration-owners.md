---
id: TASK-490
title: Replace integration checks with four contract-owned integration owners
status: In Progress
assignee:
  - claude-opus-5-5
created_date: '2026-10-17 16:13'
updated_date: '2026-10-18 03:31'
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
  - TASK-490.25
  - TASK-490.26
priority: high
type: task
ordinal: 512000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
This parent tracks the owner-approved integration-check overhaul into exactly four native Snowfall owners: `integration-access`, `integration-delivery`, `integration-assurance`, and `integration-upgrade`. The coordinator owns contract, coverage/retirement map, shared harness, scenario composition, owner registration, CI integration and acceptance verification. An implementation packet does not automatically receive its own NixOS test derivation; compatible case fragments are composed into a shared scenario. Each separately built scenario has a fresh VM/database. Same-scenario cases may use real prior records/identities through receipts; no runtime state is shared across derivations.

Normal overhaul scope is test-only. The owner explicitly authorizes only the narrowly bounded production repairs in existing TASK-490.25 and TASK-490.26; keep each in its own commit. No other production functionality, auth policy, migrations, SQLx metadata, UI/assets, or design-handoff change is authorized by this exception. Correct defects remain linked and their assertions executable.

Current milestone: TASK-490.25/.26 repairs are separately integrated at `ca46cdd5` / `136348e8`; the focused local bootstrap scenario and `integration-access` owner aggregate passed at `b1095c8f2bb2db16d79a2980bb4cd8dd0ce09d56`. Runtime evidence and an independent Postgres Jobs first-boot ordering defect (TASK-492) are recorded in the coverage map. This is only a bootstrap slice, not full Access or four-owner coverage. Access session/forms/navigation/preferences packets and Assurance triage/waiver/register/plan/verification packets are grouped into fewer scenarios with disjoint fragments; the coordinator owns composition. TASK-490.7 owns future roles and Delivery proving work. All child packets remain Backlog until their dependencies, writable paths, placement, interface and command are concrete. Keep all legacy checks until four-owner cutover conditions pass. No merge/deploy/cutover is authorized here.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The contract is adopted at docs/knowledge/testing/integration-test-contract.md with accurate implementation_status and directly affected AGENTS.md/verification guidance; as-built D1 is not accepted authority and uncertain cases require an owner decision
- [ ] #2 The coverage map assigns every accepted obligation one primary owner/scenario, independent expected result, evidence and status; records legacy retirement; states task fragments do not imply VM derivations and explains every separate VM boundary
- [ ] #3 At final cutover exactly four native Snowfall integration owners exist under checks/integration-{access,delivery,assurance,upgrade}/default.nix using { lib, inputs, pkgs, ... }; non-integration checks retain their attributes and classification
- [ ] #4 Each normal owner is a pkgs.linkFarm aggregate of all explicitly registered scenario gates and propagates every failure; meaningful WIP may be discoverable, with no placeholder owners
- [ ] #5 Shared helpers use lib/integration-test through lib.crystal-forge.makeIntegrationTestHarness { inherit pkgs inputs; }; coordinator owns composition/registration and packet case fragments are owner-local/disjoint
- [ ] #6 Legacy checks and CI are retired only after passing replacements or owner-approved scope change; correct product assertions stay executable and linked
- [ ] #7 No production code/migration/SQLx/UI/design changes except the separately committed narrow repairs explicitly authorized for TASK-490.25 and TASK-490.26; those repairs do not authorize unrelated product changes
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Organization correction recorded 2026-10-17: this task's description and AC supersede prior layout and incomplete-unexported rules. Backlog grooming only; no implementation, Luna delegation, or slice execution is claimed. Foundation dependency commit is unresolved. Foundation is blocked by two existing Rust package tests; no VM execution. All 24 children remain Backlog, none dispatch-ready. Future packets need actual dependency commit(s) and verified commands before To Do. TASK-490.7/.24 are coordinator writer tasks; TASK-490.20/.21 need finer splitting of triage, policy waiver, and closure without scope expansion now.

Historical coordination context: prior owner claude-opus-5-5 recorded the foundation worktree /home/mcamp/code/crystal-forge/TASK-490.1-integration-foundation. That historical record is not evidence of a completed foundation or current dispatch readiness.

LOCK: claude-opus-5-5 on sledge in /home/mcamp/code/crystal-forge/TASK-490.1-integration-foundation

Run one VM test at a time until measured capacity supports a higher limit. Foundation remains In Progress, blocked pending package build; dependent children remain Backlog. No foundation readiness or verification is claimed.

Corrected native-layout WIP preserved and pushed: 683c86e03ef14b37e69c00030d6d17660398324d; draft MR !335 (not merge-ready). Only integration-access is discoverable (27 total checks, previous 26 unchanged). No root tests tree/custom discovery/flake-output assembly remains. TASK-491 blocks VM execution; foundation incomplete and Luna children stay Backlog. No Luna dispatch, deployment, merge, Review, or Done transition occurred.

Foundation resumed on 2026-10-17 in the existing dedicated worktree. Integrated tested TASK-491 dependency by cherry-pick `af39284bdf16d56402f01a64c916f17948e43b02` as `a6c8922650bf7f0ff643d181422c0092cfc43242`. The exact Access scenario executed in a VM and failed at startup, preserving its assertion. Generated config evidence reveals product blockers TASK-490.25 (local PostgreSQL socket host becomes empty-authority URI) and TASK-490.26 (local auth_mode omitted; AUTH_MODE unset; route behavior not reached because DB URL failure comes first). Both are Backlog, not fixed. Coverage map now records exact evidence and marks the scenario failing, not passing. Assurance packets .20/.21 were narrowed; .27/.28/.29 split waiver, acceptance register, and verify/closure. Per-scenario fresh DB/VM rule now says task deps never share runtime outputs. All child packets remain Backlog; no Luna dispatch, merge, deploy, or application fix.

Final combined foundation commit is 39a6a54b651a8b77fc3416b73195e363a782fa2e. The focused native Access check executed a VM and failed on fresh startup; exact evidence is in docs/knowledge/testing/integration-coverage-map.md and store output `/nix/store/vcfbia521bjad2a85yca3h3n9i4m4ikf-vm-test-run-cf-it-access-lifecycle-local-bootstrap-run/`. TASK-490.25/.26 are explicit product blockers; no production changes were made. All dependent packets remain Backlog, not ready.

The milestone advanced at b1095c8f2bb2db16d79a2980bb4cd8dd0ce09d56: both the focused Access bootstrap and current Access owner aggregate passed, using one VM result. Only the local bootstrap slice is covered; all other owner obligations remain outstanding. TASK-492 records the independent Postgres Jobs first-boot role race found in the journal, outside the .25/.26 exception. The canonical description now corrects the historic scope and readiness notes.
<!-- SECTION:NOTES:END -->

---
id: TASK-490
title: Replace integration checks with four contract-owned integration owners
status: In Progress
assignee:
  - claude-opus-5-5
created_date: '2026-10-17 16:13'
updated_date: '2026-10-18 15:42'
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
This parent tracks the owner-approved overhaul into four native Snowfall owners: `integration-access`, `integration-delivery`, `integration-assurance`, and `integration-upgrade`. Coordinator owns contract, coverage/retirement map, shared harness, scenario composition, owner registration, CI integration and acceptance verification. A packet does not automatically get a NixOS derivation; compatible case fragments compose into shared scenarios. Separate scenarios have fresh VMs/databases; same-scenario cases may use earlier real records/identities through receipts; no cross-derivation runtime state is shared.

Normal overhaul scope is test-only. The owner authorized only the bounded production repairs TASK-490.25 and TASK-490.26, separately committed as `52128b3a63175b712f7a83860355fdeec658a230` and `76e514644505f232e4e47f085db801913d8449ab`. No other production, authentication-policy, migration, SQLx, UI or design-handoff changes are authorized by this exception.

After rebasing on `origin/dev` `5ae7feab8a7b372e8efe0d2dd21b6f53f0625064`, the focused Access scenario and owner aggregate passed ten cases (five bootstrap, five session) at `81d65be745cc9f9777d368900d01fe0f207cb052`. This covers only the Access bootstrap and selected session slice. TASK-490.7 owns future shared Delivery roles and proof; D1/D2 remain applicable. TASK-492 is an unrelated Postgres Jobs first-boot role race and is not an Access gate failure.

CI policy (updated 2026-10-18 by an owner-authorized early transition in TASK-490.24): implemented integration owners run automatically in GitLab (`integration-access` today, required). Retiring legacy integration checks remain available as optional manual jobs while replacements are constructed. Removing a legacy check from automatic CI does not mark its obligations covered. Delete its definition and fixtures only after passing replacement coverage is recorded, or after an explicit decision removes the underlying requirement. This supersedes the earlier instruction to keep all legacy CI scheduling unchanged until all four owners pass. It is a deliberate, temporary reduction in automatic integration coverage; the gaps are listed in the coverage map. The final four-owner cutover and legacy source retirement remain outstanding in TASK-490.24. No merge, deployment or Done transition is authorized.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The contract is adopted at docs/knowledge/testing/integration-test-contract.md with accurate implementation_status and directly affected AGENTS.md/verification guidance; as-built D1 is not accepted authority and uncertain cases require an owner decision
- [ ] #2 The coverage map assigns every accepted obligation one primary owner/scenario, independent expected result, evidence and status; records legacy retirement; states task fragments do not imply VM derivations and explains every separate VM boundary
- [ ] #3 At final cutover exactly four native Snowfall integration owners exist under checks/integration-{access,delivery,assurance,upgrade}/default.nix using { lib, inputs, pkgs, ... }; non-integration checks retain their attributes and classification
- [ ] #4 Each normal owner is a pkgs.linkFarm aggregate of all explicitly registered scenario gates and propagates every failure; meaningful WIP may be discoverable, with no placeholder owners
- [ ] #5 Shared helpers use lib/integration-test through lib.crystal-forge.makeIntegrationTestHarness { inherit pkgs inputs; }; coordinator owns composition/registration and packet case fragments are owner-local/disjoint
- [ ] #6 Implemented owners run automatically in GitLab and retiring legacy integration checks are optional manual jobs while replacements are built (early transition, TASK-490.24); legacy definitions and fixtures are deleted only after passing replacements are recorded or an explicit decision removes the requirement; correct product assertions stay executable and linked
- [ ] #7 No production code/migration/SQLx/UI/design changes except the separately committed narrow repairs explicitly authorized for TASK-490.25 and TASK-490.26; those repairs do not authorize unrelated product changes
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
## Current (2026-10-18)

Child states: TASK-490.1, .25 and .26 are Review in draft MR !335. TASK-490.2 and TASK-490.24 are In Progress (.2: session subset verified, Operator/Viewer cases unimplemented; .24: early CI transition implemented, final cutover outstanding). The remaining children are Backlog and none is dispatch-ready: they need actual dependency commits and verified commands before To Do, and TASK-490.20/.21 need finer splitting without scope expansion. TASK-490.7/.24 are coordinator writer tasks. D1 (as-built behavior is not authority) and Upgrade D2 remain applicable.

Early CI transition (TASK-490.24, owner-authorized): `integration-access` is an automatic required GitLab job; `integration`, `oidc-auth`, `niks3-cache`, `niks3-write-probe`, `niks3-ui`, `attic-metrics`, `attic-publication` and `web-ui-check` are optional manual jobs. The job script passed all ten Access cases with the VM executed newly at `2c494ac4bf8cc7155899723ee69fcf879ac49248`. Authorization, registration-policy, OIDC, broad UI/design, Delivery, Assurance and Upgrade now have no automatic integration coverage; TASK-490.2 and the other owners remain incomplete. Final four-owner cutover and legacy source retirement are outstanding. The earlier note that all legacy checks are kept until all four owners pass applies to source retirement only.

Run one VM test at a time until measured capacity supports a higher limit. LOCK: claude-opus-5-5 on sledge in /home/mcamp/code/crystal-forge/TASK-490.1-integration-foundation.

## Historical (dated records; superseded readiness statements)

2026-10-17 organization correction replaced prior layout and incomplete-unexported rules. Backlog grooming only. At that time the foundation dependency commit was unresolved, the foundation was blocked by two existing Rust package tests, no VM had executed, and all 24 children were Backlog. The prior owner claude-opus-5-5 recorded the foundation worktree /home/mcamp/code/crystal-forge/TASK-490.1-integration-foundation; that is not evidence of completed foundation.

Corrected native-layout WIP was preserved and pushed at `683c86e03ef14b37e69c00030d6d17660398324d`; draft MR !335 (not merge-ready). Only integration-access was discoverable (27 total checks, previous 26 unchanged). No root tests tree, custom discovery or flake-output assembly remains. TASK-491 then blocked VM execution.

The foundation resumed on 2026-10-17 in the existing dedicated worktree. It integrated the tested TASK-491 dependency by cherry-pick `af39284bdf16d56402f01a64c916f17948e43b02` as `a6c8922650bf7f0ff643d181422c0092cfc43242`. The exact Access scenario executed in a VM and failed at startup, preserving its assertion. Generated config evidence revealed product blockers TASK-490.25 (local PostgreSQL socket host becomes empty-authority URI) and TASK-490.26 (local auth_mode omitted; AUTH_MODE unset). Assurance packets .20/.21 were narrowed; .27/.28/.29 split waiver, acceptance register, and verify/closure. The per-scenario fresh DB/VM rule says task dependencies never share runtime outputs.

Final combined foundation commit then was `39a6a54b651a8b77fc3416b73195e363a782fa2e`; its focused Access check failed on fresh startup with evidence in `/nix/store/vcfbia521bjad2a85yca3h3n9i4m4ikf-vm-test-run-cf-it-access-lifecycle-local-bootstrap-run/`. The milestone then advanced at `b1095c8f2bb2db16d79a2980bb4cd8dd0ce09d56`: the focused Access bootstrap and the owner aggregate passed. TASK-492 records the independent Postgres Jobs first-boot role race found in the journal, outside the .25/.26 exception.
<!-- SECTION:NOTES:END -->

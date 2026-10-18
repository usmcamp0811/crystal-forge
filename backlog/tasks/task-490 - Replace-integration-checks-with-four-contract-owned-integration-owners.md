---
id: TASK-490
title: Replace integration checks with four contract-owned integration owners
status: In Progress
assignee:
  - claude-opus-5-5
created_date: '2026-10-17 16:13'
updated_date: '2026-10-18 18:22'
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
Final result: checks.x86_64-linux contains exactly FOUR TOTAL native Snowfall attributes: integration-access, integration-delivery, integration-assurance, integration-upgrade. This does not restrict verification to four commands: Rust unit and isolated PostgreSQL regressions, packaging, schema, documentation and developer utilities remain through package/app/CI mechanisms outside checks. Non-integration classification does not authorize retaining a top-level check attribute.

Coordinator owns contract, retirement map, shared harness, composition/registration, CI and acceptance verification. Keep the six-scenario starting topology and extend compatible cases rather than creating packet-specific VMs. Separate scenarios have fresh VMs/databases; later cases may use earlier real records through same-scenario receipts only. The accepted contract/current CF architecture defines product obligations; historical test inventory does not.

Owner-authorized source retirement in TASK-490.24 supersedes both retain-until-replacement and optional-manual coexistence. Nine legacy suite definitions and obsolete CI jobs are removed now; required assets and lower-level tests are relocated before deletion. Missing accepted owner coverage remains a gap, not obsolete or covered merely because old implementation is deleted. The current inventory is one implemented owner, Access, versus 27 previous attributes. Final acceptance is `nix eval .#checks.x86_64-linux --apply builtins.attrNames --json`, with exactly the four names above once their real scenarios exist. Do not add placeholders.

Access retains sixteen executed bootstrap/session/Operator-Viewer admin-denial cases; forms, preferences, notifications, OIDC, environment-scope and registration-policy work remains in the existing child tasks. Delivery/Assurance/Upgrade are unfinished. Consolidation of duplicate Rust/database selections is tracked in TASK-490.24 with exact primary-execution targets and dependencies; moving a check to a package does not by itself remove repeated work.

This remains a test-only overhaul except the previously authorized prerequisite configuration repairs TASK-490.25 and .26 (52128b3a and 76e51464). No further product, authorization-policy, schema, migration, SQLx or design change is authorized. D1/D2 and separate TASK-492/493/494 remain scoped. Existing draft MR !335; no merge/deployment/Done authorization.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The adopted integration maintenance contract and verification guidance accurately distinguish approved requirements from as-built evidence, and state exactly four TOTAL final flake check attributes
- [ ] #2 The coverage map assigns accepted obligations one primary owner/scenario, independent expected results, evidence and truthful status; distinguish executed coverage, required gaps, obsolete suites and lower-level verification retained outside checks
- [ ] #3 Final `nix eval .#checks.x86_64-linux --apply builtins.attrNames --json` returns exactly integration-access, integration-delivery, integration-assurance and integration-upgrade, with complete accepted coverage and no other attributes
- [ ] #4 Each normal native Snowfall owner is a pkgs.linkFarm aggregate of all explicitly registered scenario gates and propagates every failure; no placeholder owners
- [ ] #5 Shared helpers use lib/integration-test; coordinator composes owner-local compatible case fragments with the fewest justified scenario boundaries; no duplicate execution without distinct evidence
- [ ] #6 Delete the authorized nine legacy suites and obsolete manual jobs after relocating needed assets. Useful Rust/database/package/schema/doc/tool tests remain outside flake checks; consolidate duplicate selections or justify distinct conditions. Source retirement does not mark missing accepted obligations covered
- [ ] #7 No production/migration/SQLx/UI/design changes except the previously separately authorized TASK-490.25/.26 repairs; no unrelated product fixes
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
## Current (2026-10-18)

Child states: TASK-490.1, .25 and .26 are Review in draft MR !335. TASK-490.2 and TASK-490.24 are In Progress (.2: session and selected Operator/Viewer admin-denial cases verified; cross-environment and closed-registration cases open; .24: early CI transition implemented, final cutover outstanding). The remaining children are Backlog and none is dispatch-ready: they need actual dependency commits and verified commands before To Do, and TASK-490.20/.21 need finer splitting without scope expansion. TASK-490.7/.24 are coordinator writer tasks. D1 (as-built behavior is not authority) and Upgrade D2 remain applicable.

Early CI transition (TASK-490.24, owner-authorized): `integration-access` is an automatic required GitLab job; `integration`, `oidc-auth`, `niks3-cache`, `niks3-write-probe`, `niks3-ui`, `attic-metrics`, `attic-publication` and `web-ui-check` are optional manual jobs. The job script passed all 16 Access cases (five bootstrap, five session, six Operator/Viewer authorization) with the VM executed newly at `8a857bc872cf331a19036b97a93f2b7741e9728b`. Cross-environment scope, registration-policy, OIDC, broad UI/design, Delivery, Assurance and Upgrade now have no automatic integration coverage; TASK-490.2 and the other owners remain incomplete. Final four-owner cutover and legacy source retirement are outstanding. The earlier note that all legacy checks are kept until all four owners pass applies to source retirement only.

2026-10-18 final minimization rule (recorded in TASK-490.24, the contract and the coverage map): every remaining independent flake check or CI test job must have a distinct necessary purpose. Preserve useful assertions, assign their primary execution ownership, consolidate duplicate execution, and keep developer tools available through package or app outputs. Classification as non-integration alone does not justify a separate check. Checks kept during migration are provisional, not final. Cutover must also show no task-specific VM proliferation under the four aggregates, a reason for repeated Rust/database execution, and the final flake inventory with measured runtime and resource cost. Nothing is deleted arbitrarily before then.

Run one VM test at a time until measured capacity supports a higher limit. LOCK: claude-opus-5-5 on sledge in /home/mcamp/code/crystal-forge/TASK-490.1-integration-foundation.

## Historical (dated records; superseded readiness statements)

2026-10-17 organization correction replaced prior layout and incomplete-unexported rules. Backlog grooming only. At that time the foundation dependency commit was unresolved, the foundation was blocked by two existing Rust package tests, no VM had executed, and all 24 children were Backlog. The prior owner claude-opus-5-5 recorded the foundation worktree /home/mcamp/code/crystal-forge/TASK-490.1-integration-foundation; that is not evidence of completed foundation.

Corrected native-layout WIP was preserved and pushed at `683c86e03ef14b37e69c00030d6d17660398324d`; draft MR !335 (not merge-ready). Only integration-access was discoverable (27 total checks, previous 26 unchanged). No root tests tree, custom discovery or flake-output assembly remains. TASK-491 then blocked VM execution.

The foundation resumed on 2026-10-17 in the existing dedicated worktree. It integrated the tested TASK-491 dependency by cherry-pick `af39284bdf16d56402f01a64c916f17948e43b02` as `a6c8922650bf7f0ff643d181422c0092cfc43242`. The exact Access scenario executed in a VM and failed at startup, preserving its assertion. Generated config evidence revealed product blockers TASK-490.25 (local PostgreSQL socket host becomes empty-authority URI) and TASK-490.26 (local auth_mode omitted; AUTH_MODE unset). Assurance packets .20/.21 were narrowed; .27/.28/.29 split waiver, acceptance register, and verify/closure. The per-scenario fresh DB/VM rule says task dependencies never share runtime outputs.

Final combined foundation commit then was `39a6a54b651a8b77fc3416b73195e363a782fa2e`; its focused Access check failed on fresh startup with evidence in `/nix/store/vcfbia521bjad2a85yca3h3n9i4m4ikf-vm-test-run-cf-it-access-lifecycle-local-bootstrap-run/`. The milestone then advanced at `b1095c8f2bb2db16d79a2980bb4cd8dd0ce09d56`: the focused Access bootstrap and the owner aggregate passed. TASK-492 records the independent Postgres Jobs first-boot role race found in the journal, outside the .25/.26 exception.

2026-10-18 source retirement: previous manual-coexistence/source-retention statements are historical and superseded by the current owner instruction. Actual flake inventory drops 27 -> 1 (Access only) with 17 package verification paths retained and nine legacy suites deleted. Three future owners and their accepted coverage still need construction; source cleanup is not final suite completion.
<!-- SECTION:NOTES:END -->

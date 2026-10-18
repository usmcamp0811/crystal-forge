---
id: TASK-490.24
title: >-
  Coordinator: four-owner cutover, CI entry point, and legacy integration
  retirement
status: In Progress
assignee:
  - '@openai'
created_date: '2026-10-17 16:39'
updated_date: '2026-10-18 15:32'
labels:
  - testing
  - integration
  - coordinator
dependencies:
  - TASK-490.1
  - TASK-490.2
  - TASK-490.3
  - TASK-490.4
  - TASK-490.5
  - TASK-490.6
  - TASK-490.7
  - TASK-490.8
  - TASK-490.9
  - TASK-490.10
  - TASK-490.11
  - TASK-490.12
  - TASK-490.13
  - TASK-490.14
  - TASK-490.15
  - TASK-490.16
  - TASK-490.17
  - TASK-490.18
  - TASK-490.19
  - TASK-490.20
  - TASK-490.21
  - TASK-490.22
  - TASK-490.23
  - TASK-490.25
  - TASK-490.26
  - TASK-490.27
  - TASK-490.28
  - TASK-490.29
modified_files:
  - checks/integration-access/default.nix
  - checks/integration-delivery/default.nix
  - checks/integration-assurance/default.nix
  - checks/integration-upgrade/default.nix
  - .gitlab-ci.yml
  - docs/knowledge/testing/integration-coverage-map.md
parent_task_id: TASK-490
priority: high
type: task
ordinal: 536000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Coordinator-owned cutover task. Owns the four native Snowfall owner registrations, CI entry point, legacy-gate retirement, coverage/retirement map, and acceptance verification. Do not delegate owner registration/shared-library/CI edits or add placeholder owners.

The TASK-490.25/.26 repairs are separately integrated at rebased commits `52128b3a63175b712f7a83860355fdeec658a230` and `76e514644505f232e4e47f085db801913d8449ab`. After rebasing on `origin/dev` `5ae7feab8a7b372e8efe0d2dd21b6f53f0625064`, the focused Access scenario and owner aggregate passed at `bba718befbc183863781656df2df272e5f735d5a`; evidence is in the coverage map. This proves only the bootstrap slice. TASK-490.7 role implementation/Delivery proving remains pending. All child packets remain Backlog and none is ready while TASK-490.1 remains In Progress and authority decisions remain unresolved.

Cutover is not ready: only Access exists and it covers a subset. Delivery, Assurance and Upgrade remain incomplete. Keep legacy checks/CI until all four owner aggregates pass the accepted map or the owner approves a scope change. D1 and Upgrade D2 remain applicable. TASK-492 is separate and outside the .25/.26 authorization. No merge, deployment, or cutover is authorized.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 At final cutover exactly four native checks/integration-{access,delivery,assurance,upgrade}/default.nix owners use { lib, inputs, pkgs, ... }; each normal pkgs.linkFarm aggregate executes ALL registered gates and its complete required set, propagating every failure
- [ ] #2 CI runs the four owners through checks.x86_64-linux and checks the four-name allowlist in the existing entry point, without a new validation service
- [ ] #3 Every retired legacy check/job has a passing replacement recorded in the coverage map or explicit owner-approved scope change; failing product assertions remain executable and linked
- [ ] #4 Non-integration checks retain their own attributes and explicit classification
- [ ] #5 Runtime, peak resources, exact commands, revisions, real VM outcomes and .run evidence are recorded per owner
- [ ] #6 Coordinator explicitly registers child scenarios and maintains shared lib/CI/docs; scripts/fixtures/assets are owner-local, no root tests/, legacyPackages.integration, mkOwner, custom discovery, complete=true export switch or placeholder owners; meaningful WIP discovery is permitted before cutover
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
2026-10-17 organization correction replaces old registration/layout/commands and incomplete-unexported rule. Coordinator writer, not Luna. Only Access exists now; remaining native owner default.nix files require meaningful scenarios, no placeholders. Foundation dependency commit unresolved; foundation blocked by two existing Rust package tests; no VM execution. All children Backlog, none ready. Future packets need actual dependency commit(s) and verified commands before To Do. TASK-490.20/.21 finer splitting remains a dispatch blocker without scope expansion now. D1/as-built is not authority; uncertain cases await owner decision. No delegation or executed slice is claimed.

Cutover dependencies expanded 2026-10-17 to include the two product blockers discovered by the correct Access foundation slice (TASK-490.25/.26) and the decomposed Assurance work packets (.27-.29). Passing or explicit owner scope approval is still required before retiring the legacy gates. No dependent task is ready.

At tested source revision b1095c8f2bb2db16d79a2980bb4cd8dd0ce09d56, focused Access VM and `nix build .#checks.x86_64-linux.integration-access -L` both passed. Current runtime evidence is `/nix/store/xxvi4m1s6a7ajm4al64sqqb7qvhhidjz-vm-test-run-cf-it-access-lifecycle-local-bootstrap-run/`. Only the bootstrap slice is covered. TASK-492 records the unrelated one-shot Postgres Jobs first-boot role race; no Access assertion failed.

The Access focused scenario and owner aggregate were repeated at final documentation/comment revision `ade5080cd84c7be057b8f0a9d9c490ed8b1fd3d5`; both pass. The evidence output is `/nix/store/0aavcw4vk32c83gljg8y4mz7gi5pqrn4-vm-test-run-cf-it-access-lifecycle-local-bootstrap-run/`. This repeat verifies the final exact head; it is not evidence for any missing Access contract or other owner.

After rebase, the current origin/dev base is `5ae7feab8a7b372e8efe0d2dd21b6f53f0625064`. Both focused Access and owner aggregate passed at rebased source `bba718befbc183863781656df2df272e5f735d5a`; evidence `/nix/store/riky1mm0nn185wb26abw7zzshnbml0fp-vm-test-run-cf-it-access-lifecycle-local-bootstrap-run/`. Cutover remains blocked by incomplete Delivery/Assurance/Upgrade owners and D1/D2, not by the Access bootstrap.
<!-- SECTION:NOTES:END -->

---
id: TASK-490
title: Replace integration checks with four contract-owned integration owners
status: In Progress
assignee:
  - claude-opus-5-5
created_date: '2026-10-17 16:13'
updated_date: '2026-10-17 17:24'
labels:
  - testing
  - integration
dependencies: []
references:
  - /home/mcamp/code/crystal-forge/crystal-forge-integration-test-contract.md
  - AGENTS.md
  - docs/agents/verification.md
  - docs/knowledge/testing/flake-checks.md
  - TASK-490.25
  - TASK-490.26
priority: high
type: task
ordinal: 512000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The current integration layer mixes about 14 VM checks and a 26.9k-line browser suite. 105 of 181 browser steps are route-mocked; OIDC is not completed end to end, CVE scanning uses a fake runner, deploy/rollback is excluded, and a real old-version upgrade is missing. The authoritative browser job is allow_failure. This parent tracks the owner-approved test-only overhaul into four contract owners: integration-access, integration-delivery, integration-assurance, integration-upgrade.

Contract source: /home/mcamp/code/crystal-forge/crystal-forge-integration-test-contract.md, to be adopted at docs/knowledge/testing/integration-test-contract.md. The organization correction below supersedes prior layout proposals. As-built documentation (D1) is implementation evidence, not accepted authority. Uncertain expected behavior is blocked pending an explicit owner decision; do not infer acceptance from implementation.

ORGANIZATION: native Snowfall owners live at checks/integration-{access,delivery,assurance,upgrade}/default.nix with signatures { lib, inputs, pkgs, ... }. Scenario scripts, fixtures, and assets live under their respective owner directories. Shared helpers live at lib/integration-test and are obtained through lib.crystal-forge.makeIntegrationTestHarness { pkgs, inputs }. No root tests/ tree, legacyPackages.integration export, mkOwner, or custom discovery. Access has an explicit coordinator-owned scenarios attrset. Each normal owner derivation uses pkgs.linkFarm to aggregate ALL registered scenario gates and propagate failures. Focused foundation target: checks.x86_64-linux.integration-access.scenarios.lifecycle-local-bootstrap, with .run for evidence. Meaningful WIP checks may be discoverable. Only Access exists at this stage; other owner default.nix files are added only with meaningful scenarios, never placeholders. Exactly four integration owners is the final cutover inventory, not a foundation export precondition.

SINGLE WRITER: the coordinator owns every owner default.nix, lib/integration-test/**, flake/CI wiring, canonical contract, coverage map, AGENTS.md and directly affected agent/testing docs. Children write disjoint named scenario files and owner-local fixtures/assets; integration requires explicit coordinator registration. TASK-490.7 and TASK-490.24 are coordinator-written work, not Luna packets.

STRICT SCOPE: application functionality is read-only. Allowed: integration test code, test-only fixtures/configs/helpers/browser tooling, native owner registrations and required execution wiring, pinned test dependencies without application dependency changes, existing integration CI entry point at cutover, contract, coverage map, directly affected testing docs/agent links, and task records. Forbidden: production Rust/UI/assets/styles/APIs/authz/policy/deployment logic/migrations/SQLx metadata/module semantics, production selectors, debug endpoints, auth bypasses, mock-success paths, patched application derivations, runtime dependency changes, and docs/design/CrystalForge/ edits. Rust unit tests stay. Product defects retain executable failing assertions and become linked Backlog bugs.

Overhaul branch: TASK-490.1-integration-foundation; designated historical base origin/dev 33de73b013b735304dfac9c69745a074a3e76ebd. This base is not a foundation dependency commit. Child commits are integrated by the coordinator.

READINESS: TASK-491's tested package repair `af39284bdf16d56402f01a64c916f17948e43b02` is integrated. Final combined commit `fc930335878279ac426510359c039dfd8961dc13` ran the Access VM and failed on normal module configuration before migrations/HTTP. Evidence is `/nix/store/1d5178gxpdv7j4d20dddddr7ik0pq0p3-vm-test-run-cf-it-access-lifecycle-local-bootstrap-run/`. TASK-490.25/.26 record the observed product configuration defects. No product fix is authorized or implemented. All child scenario packets remain Backlog; no Luna dispatch. TASK-490.20/.21 were narrowed; .27/.28/.29 split finding waiver, acceptance register and POA&M verification/closure. Every scenario must create its own real starting state in its fresh VM; no cross-scenario runtime reuse. Final cutover requires complete owner runs to pass or explicit owner scope change.

Prior owner authorization covers this overhaul only and permits dependency-ready task selection and implementation preflight. It does not override these blockers or authorize merging, deploying, unrelated work, product fixes, or marking the overhaul Done without final approval.
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

2026-10-17 resumed foundation update: TASK-491 tested repair `af39284bdf16d56402f01a64c916f17948e43b02` was cherry-picked; follow-up packet/documentation corrections were committed as `39a6a54b651a8b77fc3416b73195e363a782fa2e`. That final commit ran the focused Access VM and failed before migrations: generated `/run/postgresql` host produced an empty-host URI; generated auth_mode was absent and AUTH_MODE was unset. Evidence, exact expected/observed result and store path are in the coverage map. Product bugs TASK-490.25/.26 are Backlog; no fix is implemented. Scenario packet boundaries were corrected; runtime state is not shared between scenarios. All children remain Backlog and no owner passes.
Final clean verification supersedes the earlier recorded revision: commit `fc930335878279ac426510359c039dfd8961dc13`; exact VM evidence `/nix/store/1d5178gxpdv7j4d20dddddr7ik0pq0p3-vm-test-run-cf-it-access-lifecycle-local-bootstrap-run/`. The same startup assertion still fails. Product defects remain unresolved.
<!-- SECTION:NOTES:END -->

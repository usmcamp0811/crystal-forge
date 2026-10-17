---
id: TASK-490.1
title: >-
  Integration overhaul foundation: contract, coverage map, shared harness, and
  Access bootstrap slice
status: In Progress
assignee:
  - claude-opus-5-5
created_date: '2026-10-17 16:13'
updated_date: '2026-10-17 18:37'
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
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/335'
parent_task_id: TASK-490
priority: high
type: task
ordinal: 513000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Problem: children need an adopted integration contract, coverage/retirement map, a concrete shared harness, and a verified registration interface. Goal: establish those foundations and prove one Access slice: fresh server/database startup using normal module configuration, supported first-admin bootstrap, production-UI browser sign-in, and authoritative reload/read. Test-only scope from TASK-490 applies; docs/design/CrystalForge/ is read-only.

Corrected architecture: native checks/integration-access/default.nix uses Snowfall signature { lib, inputs, pkgs, ... }, an explicit coordinator-owned scenarios attrset, and a normal pkgs.linkFarm aggregate of ALL registered gates. The focused slice is checks.x86_64-linux.integration-access.scenarios.lifecycle-local-bootstrap; .run exposes evidence. Scenario .nix/.py/.cjs scripts, fixtures, and assets belong under checks/integration-access/. Shared helpers belong under lib/integration-test and use lib.crystal-forge.makeIntegrationTestHarness { pkgs, inputs }. No root tests/, legacyPackages.integration, mkOwner, or custom discovery. Meaningful WIP checks can be discoverable; no incomplete-unexported rule. Only Access exists now; create Delivery/Assurance/Upgrade default.nix only when each has meaningful scenarios, never placeholders.

Coordinator owns lib/integration-test/**, owner default.nix registration, flake/CI wiring, contract, coverage map and affected AGENTS.md/agent/testing docs. Children receive disjoint scenario files and owner-local fixtures/assets; their integration requires explicit coordinator registration.

Harness scope: at most five roles (controller, services, builderA, builderB, targetA), instantiate only needed roles; immutable inputs only are shared. Select production cf-server-drv with embedded UI through the normal module. Provide deterministic facts plus runtime receipts, bounded waits, fault injection, pinned browser/render profile, failure artifacts, and exact documented helper signatures. Add roles only with a proving scenario, not speculative abstractions. No generic DSL, generated registry, coverage database, new CI service, legacy removal, product/module fixes, or design edits.

Authority: adopt accepted owner requirements; D1 as-built docs are evidence only, not accepted authority. Uncertain cases remain blocked pending owner decision. Contract implementation_status stays partial until evidence supports more.

Verification candidates (not verified): nix eval .#checks.x86_64-linux.integration-access.scenarios --apply builtins.attrNames; nix build .#checks.x86_64-linux.integration-access.scenarios.lifecycle-local-bootstrap -L; nix build .#checks.x86_64-linux.integration-access.scenarios.lifecycle-local-bootstrap.run -L for evidence; nix build .#checks.x86_64-linux.integration-access -L for all registered gates. Verify native check discovery and no placeholder owners. Real VM execution, not evaluation/build alone, must be evidenced with actual outcome and command. Preserve correct failing assertions and link product bugs.

READINESS: In Progress, blocked pending package build by two existing Rust package tests; no VM execution. The actual foundation dependency commit is unresolved. Historical base origin/dev 33de73b013b735304dfac9c69745a074a3e76ebd is not that commit. Future child packets need actual dependency commit(s), verified commands, implemented helper signatures, exact paths, fixtures and registration before To Do. No delegation or executed slice is claimed. Risk: medium, shared interface and VM cost. Live preview not applicable: no browser-visible behavior change.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Contract is adopted at docs/knowledge/testing/integration-test-contract.md with implementation_status partial, adopted-versus-implemented distinction, and affected AGENTS.md/agent verification links; D1 is not authority
- [ ] #2 Coverage map records accepted obligation ID, owner, scenario, independent expected result, evidence and status, plus retirement disposition for every legacy check and CI job
- [ ] #3 lib/integration-test exposes lib.crystal-forge.makeIntegrationTestHarness { pkgs, inputs } with documented concrete roles, production package selection, facts/receipts, bounded waits, fault injection, pinned rendering and failure artifacts
- [x] #4 checks/integration-access/default.nix uses { lib, inputs, pkgs, ... }, explicitly registers lifecycle-local-bootstrap and aggregates ALL registered gates with pkgs.linkFarm; scripts/fixtures/assets stay owner-local
- [ ] #5 Focused checks.x86_64-linux.integration-access.scenarios.lifecycle-local-bootstrap executes in a real VM with baseline updating disabled and failure propagation; exact command, revision, outcome and .run evidence are recorded, including linked defects where applicable
- [x] #6 Native discovery exposes meaningful Access WIP without requiring completion; no root tests/, legacyPackages.integration, mkOwner, custom discovery or placeholder Delivery/Assurance/Upgrade owners are added; legacy checks remain until cutover
- [ ] #7 Future bounded child packets record actual foundation/dependency commit(s), implemented interfaces, disjoint paths, fixtures, explicit coordinator registration and verified commands before To Do; until then all children remain Backlog
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
2026-10-17 organization correction: native owner directories and lib/integration-test replace the previous layout and completion-only export proposal. Backlog grooming only. Foundation is blocked by two existing Rust package tests; no VM execution or Luna delegation occurred. Actual foundation dependency commit remains unresolved; do not substitute the historical base hash. All children remain Backlog and none is ready. Future packets require actual dependency commit(s) and verified commands before To Do. Uncertain D1/as-built cases require owner decision.

Historical preflight (not a current lock): branch/worktree TASK-490.1-integration-foundation at /home/mcamp/code/crystal-forge/TASK-490.1-integration-foundation was recorded from origin/dev 33de73b013b735304dfac9c69745a074a3e76ebd. Prior notes reported preserved pre-existing dev files, no main worktree, /dev/kvm present, and no live-preview requirement for this test-only task. This correction does not verify those historical environment observations.

Task status correction: foundation remains In Progress under the existing lock and worktree. Layout correction does not retire or restart this active implementation task. Dependent Luna packets remain Backlog pending verified dependency commits. Snowfall helper signature verified during evaluation: lib.crystal-forge.makeIntegrationTestHarness { inherit pkgs inputs; } (the check passes inputs because Snowfall's library-loader inputs do not include self).

LOCK: claude-opus-5-5 on sledge in /home/mcamp/code/crystal-forge/TASK-490.1-integration-foundation

Run one VM test at a time until measured capacity supports a higher limit. Foundation remains In Progress, blocked pending package build; dependent children remain Backlog. No foundation readiness or verification is claimed.

Native Snowfall correction verification (2026-10-17): nix eval --json .#checks.x86_64-linux --apply builtins.attrNames succeeded: 27 attributes, exactly the prior 26 plus integration-access. nix eval --raw .#checks.x86_64-linux.integration-access.scenarios.lifecycle-local-bootstrap.drvPath and the normal owner .drvPath succeeded. nix build .#checks.x86_64-linux.integration-access --dry-run --no-link succeeded and resolves the unchanged cf-server package derivation /nix/store/9pihfg4v8mp6rb4qnp148acv5cqsrb43-cf-server-0.3.0.drv (previous two failures recorded in TASK-491). Dry run does not execute a VM. No third blind retry or disabled package test was used.

Static verification succeeded via nix develop: node --check on lib/integration-test/browser/cf-browser.cjs and checks/integration-access/scenarios/lifecycle-local-bootstrap.cjs; Python ast.parse for lib/integration-test/driver.py and checks/integration-access/scenarios/lifecycle-local-bootstrap.py. Documentation validation passed (58 tests, 191 concepts, 216 Mermaid blocks); final rerun follows final edits. git diff --check passed. flake.nix exactly matches base again. Removed only overhaul-owned empty root tests directories. Production packages/modules/migrations/SQLx/UI/design and existing CI gates unchanged.

BLOCKED: TASK-491 prevents real VM execution of the foundation slice. Bootstrap/session behavior remains unverified. No children are ready; all Luna packets need recorded tested dependency commits and executable interface evidence before To Do. No Luna delegation occurred. Foundation is In Progress, not Review/Done.

Preserved corrected native Snowfall WIP in commit 683c86e03ef14b37e69c00030d6d17660398324d on TASK-490.1-integration-foundation, pushed to origin. Draft MR !335: https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/335 . This is NOT a tested foundation dependency commit: no VM slice executed because TASK-491 blocks the unchanged server package. Status remains In Progress, lock retained; no Review/Done or ready Luna packets. Final okf-knowledge passed after registering the new Access README in the existing OKF manifest (58 tests, 191 concepts, 216 Mermaid blocks). No full flake check was run.

TASK-491 prerequisite repaired in commit af39284bdf16d56402f01a64c916f17948e43b02; draft/normal MR !336: https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/336. The normal production server package build now passes its full check phase (1794 passed, 0 failed, 682 ignored). This does NOT establish the Access VM scenario. TASK-490.1's focused scenario has not yet been rerun at this prerequisite commit; remains In Progress, blocked on executing its real VM/browser slice. TASK-491 is in Review; do not select/delegate dependent packets until coordinator integrates the prerequisite commit and verifies the slice.

Correction after TASK-491 prerequisite review: the server package build blocker is now resolved by TASK-491 commit af39284bdf16d56402f01a64c916f17948e43b02 (MR !336, status Review). The focused Access scenario still has not been executed on that commit. TASK-490.1's prior note saying it is blocked on package build is historical; current next action is coordinator to integrate/rebase the tested prerequisite and execute the real Access VM/browser slice. No owner result is implied and child packets remain Backlog.
<!-- SECTION:NOTES:END -->

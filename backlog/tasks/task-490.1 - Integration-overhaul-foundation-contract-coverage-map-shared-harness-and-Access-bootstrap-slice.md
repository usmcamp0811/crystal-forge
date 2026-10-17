---
id: TASK-490.1
title: >-
  Integration overhaul foundation: contract, coverage map, shared harness, and
  Access bootstrap slice
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
- [x] #2 Coverage map records accepted obligation ID, owner, scenario, independent expected result, evidence and status, plus retirement disposition for every legacy check and CI job
- [ ] #3 lib/integration-test exposes lib.crystal-forge.makeIntegrationTestHarness { pkgs, inputs } with documented concrete roles, production package selection, facts/receipts, bounded waits, fault injection, pinned rendering and failure artifacts
- [x] #4 checks/integration-access/default.nix uses { lib, inputs, pkgs, ... }, explicitly registers lifecycle-local-bootstrap and aggregates ALL registered gates with pkgs.linkFarm; scripts/fixtures/assets stay owner-local
- [x] #5 Focused checks.x86_64-linux.integration-access.scenarios.lifecycle-local-bootstrap executes in a real VM with baseline updating disabled and failure propagation; exact command, revision, outcome and .run evidence are recorded, including linked defects where applicable
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

Native Snowfall correction verification: `nix eval --json .#checks.x86_64-linux --apply builtins.attrNames` succeeded with the prior 26 checks plus integration-access. Normal-owner and focused-scenario drvPath evaluation succeeded. `nix build .#checks.x86_64-linux.integration-access --dry-run --no-link` resolves the unchanged production server derivation; it does not execute a VM. flake.nix matches base. JavaScript syntax and Python ast.parse checks passed through nix develop. Documentation validation passed (58 tests, 191 concepts, 216 Mermaid blocks). git diff --check passed.

Foundation execution update: TASK-491 test-only prerequisite `af39284bdf16d56402f01a64c916f17948e43b02` is integrated as `a6c8922650bf7f0ff643d181422c0092cfc43242`. The exact focused Access command executed a real VM and failed on the correct server-startup assertion before migrations/HTTP. Evidence: `/nix/store/yfc7pkz5zm8k29b82zzfhm2i8f1gn348-vm-test-run-cf-it-access-lifecycle-local-bootstrap-run/`. The redacted generated config records database.host=/run/postgresql, missing server.auth_mode and systemd AUTH_MODE=unset. The server exits `both host and hostaddr are missing`. Product bugs TASK-490.25/.26 track this; no auth override or production change.

Coverage map now records all obligations/legacy checks/CI dispositions and these actual failure states. Contract/map explicitly require fresh VM/DB per scenario and prohibit runtime-output reuse. Assurance packets .20/.21 are narrowed; .27/.28/.29 split distinct states/authority. All remain Backlog. `okf-knowledge` will be rerun after final doc edits. The initial controller-only foundation does not implement services/builder/target roles; this role completion/proving task remains coordinator-owned TASK-490.7. Foundation remains In Progress; no dependent packet is ready and no Luna work was dispatched.
<!-- SECTION:NOTES:END -->

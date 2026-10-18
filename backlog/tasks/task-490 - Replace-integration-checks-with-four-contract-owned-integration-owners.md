---
id: TASK-490
title: Replace integration checks with four contract-owned integration owners
status: In Progress
assignee:
  - claude-opus-5-5
created_date: '2026-10-17 16:13'
updated_date: '2026-10-18 21:06'
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
Build a small, trustworthy product-testing architecture with exactly four TOTAL native Snowfall check attributes: integration-access, integration-delivery, integration-assurance, integration-upgrade. Minimize unnecessary suites, VM boots, duplicate tests/builds and CI overhead while proving accepted CF contracts. Lower-level tests may run outside these attributes, but not through one public verification wrapper per old suite.

The nine legacy suites are retired and all fifteen packages/verify-* wrappers are eliminated. Unique assertions now execute through ordinary core/metadata/OSCAL component builds, the existing consolidated isolated database workflow, direct module/runtime/schema checks and developer-tool selftests. No aggregator framework, registry or fifth check. Current counts: 1 check, 0 verify outputs, 28 public package attributes (was 43), 9 expanded CI jobs (was 14). The three future owners are absent, not placeholders.

The existing coverage map inventories current HTTP methods/auth/mutations, Dioxus interactions, production configuration, Git/evaluation, builders, agents, caches, scans/compliance/remediation, interchange/history and upgrades against accepted requirements and approved designs. It has 79 obligation rows in 12 families: 5 rows have executed subsets, 65 planned, 9 blocked by specific decisions. This is not endpoint-percentage coverage or complete row coverage. Current implementation discovers surfaces; it does not approve its own expected behavior.

At clean c254b1c54e0f40adfaa9d05b5766cfeffae9bb89 the normal Access CI path passed 23 cases with a newly executed single controller VM: original 16 unchanged, then 7 real Admin-form cases with reload and independent SQL. The consolidated DB workflow also exited 0 with 515 selected Rust cases and both populated migration rehearsals. Normal core/protocol/config, module, schema and tooling checks passed. Exact metrics/evidence and remaining duplication are in TASK-490.24; no total runtime speedup claim.

The next shared-role milestone is .7: real services, separately authorized API-only builders and target, with an initial Git->evaluation->actual build proof in one Delivery lifecycle. .3 remains unfinished for environment CRUD/nonempty assignments and exact missing failure-state requirements; .2 environment-scope/registration decisions remain specific. OIDC, notifications, Delivery, Assurance, Upgrade and visual comparisons are not implied by the passing Access slice. Keep the six initial scenario boundaries and justify any new isolation.

Final acceptance requires all four exact names and complete passing accepted coverage or an explicit scope disposition, not a green partial pipeline. No production Rust/UI/API/schema/design/module-behavior change, merge, deployment or Done claim. Prior .25/.26 configuration repairs remain the only authorized production exception.
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
## Current verified iteration

Existing TASK-490.1-integration-foundation worktree and draft MR !335. TASK-490.24 owns consolidation and inventory; TASK-490.3 owns the executed bounded Admin slice. .1/.25/.26 remain Review; .2/.3/.24 remain In Progress. Final owners/coverage/minimality are incomplete.

All 15 verify outputs removed. Public package count 43 -> 28; expanded CI jobs 14 -> 9; checks remain [integration-access]. Unique protection belongs to ordinary components, direct CI/module/tool mechanisms and the isolated database runner. Six ignored overlaps, 94 repeated cf-server library selections and 2 repeated integration invocations removed. Core now runs all 94 cf-config tests instead of the old standalone CA-only 13, so gross duplicate removal is not a 102-test net whole-pipeline reduction. Broader lower-level dependency union including direct-CI tools/templates 7563 -> 7547 (net 16 recipes); no overall runtime speedup claim.

At clean c254b1c54e0f40adfaa9d05b5766cfeffae9bb89: normal Access CI command exited 0 with 23/23 cases, original 16 first and unchanged. Seven real Admin UI phases have reload and SQL evidence. One VM per run; final driver 50.72s, startup 10.5s reused once; 4GiB configured, peak memory unmeasured. Evidence /nix/store/nv2rpc4szk7525aa9fpb4289nz1hpbqv-vm-test-run-cf-it-access-lifecycle-local-bootstrap-run. .3 only AC5/6 checked; remaining form requirements stay open.

Full canonical DB command also exited 0 at c254b1c5: nix run .#devScripts.state-machine-test -- -t=false up. 227 Rust summaries total 515 passed, 0 failed, 0 ignored; 284 selected ignored-library tests plus 231 integration cases in 14 targets and both populated migration rehearsals. One invocation-owned isolated PG cluster, superuser/createdb and statistics preload verified; cluster shut down/cleaned. Runtime 18m26s; /tmp/opencode/task490-db-final.log and .status=0. Earlier fixture/env and supervisor-status issues were fixed without changing product behavior or weakening tests. Core/protocol/config, metadata/OSCAL, module/runtime/XCCDF and tool tests passed too.

Coverage inventory: 79 rows, 12 families, 5 rows with covered subsets, 65 planned, 9 decision-blocked. Actor/start/action/independent expectation/observation/status recorded. Some discovered cases need assignment within existing packets. No new task family or code-defined acceptance authority.

Next .7 is controller-ready but unimplemented: real roles and actual build proof, then extend the same Delivery scenario. .3 environment success cases are also runnable. Exact pending decisions apply only to environment scope/registration, specified failed-save/presentation states, remaining transport/interchange policies and real supported old-release compatibility. No blanket D1/startup blocker.

Remaining minimality: separate CVE/dashboard workflow/cache contexts, identical CA inputs and safe batching of exact Rust invocations after process-global isolation audit. Current partial pipeline is not full product coverage.

## Historical
Legacy suites retired at 0105e512. The one-for-one verify relocation was rejected and removed this iteration. Prior preservation/manual-coexistence and pending-DB notes are superseded. .25/.26 enabled initial bootstrap; earlier sessions/role proofs had 16 cases. TASK-492/493/494 remain separate. No merge/deploy/Done.
<!-- SECTION:NOTES:END -->

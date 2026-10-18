---
id: TASK-490
title: Replace integration checks with four contract-owned integration owners
status: In Progress
assignee:
  - claude-opus-5-5
created_date: '2026-10-17 16:13'
updated_date: '2026-10-18 20:48'
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
Clean-slate product testing overhaul. Final checks.x86_64-linux contains exactly FOUR TOTAL native Snowfall owners: integration-access, integration-delivery, integration-assurance, integration-upgrade. Minimize meaningful scenarios, duplicate tests/builds and CI overhead while proving accepted CF contracts. Rust/unit/isolated-database/module/schema/doc/tool tests may run outside these attributes, but not as one public wrapper per historical suite.

The preservation-first architecture is superseded. Nine legacy suites are retired and all fifteen packages/verify-* wrappers are eliminated. Unique assertions now execute in ordinary component builds, the existing consolidated DB workflow and explicit CI/module/tool commands; no new aggregator framework, registry or fifth check. Counts currently: one check, zero verify packages,28public package attributes (was43),9expanded CI jobs (was14). The three future owners remain absent, not placeholders.

Coverage is defined from accepted requirements/approved design and the current product surface, not old suite names. The existing integration coverage map inventories HTTP methods/routes/auth/mutations, Dioxus interactions, production config, Git/evaluation, builders, agents/deployment, caches, scans/compliance/remediation, interchange/history and upgrades.79obligation rows across12families: five have executed subsets (23Access cases),65planned,9decisionblocked. These are not endpoint percentages or full row completion.

Access proves bootstrap, sessions, selected Operator/Viewer denials and a bounded Admin-form subset. At clean c254b1c54e0f40adfaa9d05b5766cfeffae9bb89 the normal owner CI path passed23cases with a new single controller VM; original16 unchanged first. .3 remains In Progress for environment CRUD/nonempty bindings and exact missing failed-save states. .7 is next shared-role/real Git->evaluation->build foundation for Delivery and later OIDC/notifications. It is not blocked by historical Access failures or blanket D1. All later accepted work remains in existing children and six initial scenario boundaries; no one-VM-per-packet design.

Final acceptance requires the four exact attrNames and complete passing accepted coverage or approved scope dispositions, not a green partial pipeline. Current inventory remains [integration-access]. Critical DB tests retain fresh isolated PG privileges, schema, transaction and concurrency assertions; normal Rust package tests remain. Source/recipe and repeated-work measurements are recorded in .24, with a final canonical DB run still requiring its actual status before full-pass claims. No product/UI/API/schema/design changes, merge or deployment. Prior .25/.26 repairs remain the only authorized production exception.
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
## Current clean-slate iteration

Active implementation uses the existing TASK-490.1-integration-foundation branch/worktree and draft !335. .24 owns lower-level consolidation and current product inventory; .3 bounded Admin forms are implemented/executed. .1/.25/.26 remain Review; .2/.3/.24 remain In Progress. Future owners and final acceptance are incomplete.

All15verify public outputs gone; no renamed wrappers. Unique tests now belong to normal core/metadata/OSCAL builds, direct module/runtime/tool CI, and the existing isolated state-machine database workflow. Six ignored overlaps,94 repeated normal-library selections and2 repeated integration cases removed. Counts43->28package attrs,15->0verify attrs,14->9expandedjobs; checkcount1->1. Broader recursive derivation recipe union including required direct runtime inputs7563->7547; no total runtime speedup claim. Remaining CVE/dashboard consolidation/cache reuse and identical CA provenance input dedup are explicit .24 follow-ups, not hidden as finished.

Accepted product inventory79rows/12families now has5rows with covered subsets,65planned and9decisionblocked. Every row records actor/start/action/independent result/observation/status and primary owner/scenario. Current code is discovery evidence, not approval. Some newly discovered cases need assignment within existing packets before implementation.

Normal Access CI path passed23/23 at clean c254b1c54e0f40adfaa9d05b5766cfeffae9bb89. Original16 preserved, seven new real Admin form cases. One controller VM perrun; successfuldriver50.72s and startup10.5s reusedonce. No extra VMderivation. Evidence /nix/store/nv2rpc4szk7525aa9fpb4289nz1hpbqv-vm-test-run-cf-it-access-lifecycle-local-bootstrap-run. .3 has only AC5/6checked; remainingforms/environment/failure states open.

Normalcore build/protocols/configtests and module/runtime/schema/tool commands passed. DB prefix, focused fixed worker,116tailtests and51broad groups passed in isolated runs. Final full canonical .state-machine-test run status /tmp/opencode/task490-db-final.status must be inspected before claiming complete DB verification. Supervisor-child failure is independently propagated via invocation-owned receipt. .24 remains In Progress.

Next runnable shared infrastructure work is .7 real service/builder/target roles and commit->eval->actualbuild proof, extending one Delivery lifecycle. Current controller dependency is verified; no old startup failure or blanket D1 gate. .3 environment success cases also runnable. Exact pending decisions apply only to environment-scope/registration, selected failure/presentation states, remaining transport/interchange policies and genuine supported old-version compatibility.

## Historical
Nine legacy suites source-retired0105e512. The17lower-level relocation and15verify wrappers were rejected as final architecture and removed this iteration. Prior manual-coexistence and preservation-until-four-owner rules are superseded. .25/.26 repaired startup under explicit prior authority; bootstrap/session/role evidence progressed10->16cases before this23-case slice. TASK-492/493/494 remain unrelated recorded issues. No merge/deploy/Done.
<!-- SECTION:NOTES:END -->

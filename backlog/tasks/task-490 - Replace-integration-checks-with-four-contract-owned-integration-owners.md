---
id: TASK-490
title: Replace integration checks with four contract-owned integration owners
status: In Progress
assignee:
  - '@openai'
created_date: '2026-10-17 16:13'
updated_date: '2026-10-19 19:27'
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
  - TASK-495
  - TASK-496
priority: high
type: task
ordinal: 512000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Build a small, trustworthy product-testing architecture with exactly four final native Snowfall owners: integration-access, integration-delivery, integration-assurance and integration-upgrade. Permanent files and scenario boundaries follow stable product behavior, not tracking tasks or implementing agents. Minimize duplicate work and justify configuration/isolation boundaries.

The nine superseded suites and fifteen verify-* wrappers remain retired. Unique assertions run in ordinary components, direct module/runtime/schema/tool CI and the existing isolated PostgreSQL workflow. No replacement registry, aggregator, DSL or fifth check is permitted. Access and Delivery now have meaningful native owners; Assurance and Upgrade are absent, not placeholders.

Verified code bea0c299 passed hosted pipeline 2964502717 with all eight required jobs successful. Access preserves all 23 existing cases in one controller/database lifecycle and uses sessions/authorization/admin-forms modules. Delivery passed eight cases in one five-role lifecycle: authored Git commit, exact native evaluation/attempt/derivation, authenticated API-only builder realization, independently cold nonce output, signed local publication, distinct builder sessions and real target periodic reporting. This is not complete coverage of either owner.

The existing 79-row catalog has 11 rows with passed subsets, 59 planned and nine decision-blocked. Known failing runtime variants remain explicit; passing subsets are not percentages or complete rows. Accepted requirements and designs define expectations. Current code locates mechanisms only.

The corrective iteration verified true offline outer/nested Nix acquisition settings, separate controlled preparation, declared absolute Bash, root PostgreSQL privilege separation, short invocation-owned sockets and explicit XCCDF realization. Hosted database execution passed 515 cases and both populated migration rehearsals. Local outbound-denied verification and its host-daemon limitation are recorded in the coverage map.

The owner separately authorized TASK-495 after offline=true exposed an unsupported legacy Nix flag. It was developed in its own worktree, committed independently and is now Review in draft MR !335. offline=true and all Delivery assertions remain. Prior .25/.26 repairs remain integrated. No other production/UI/API/schema/design repair is authorized; TASK-496 remains Backlog.

Extended Access and Delivery, all Assurance, the supported Upgrade transition and final minimization remain open. Cold raw-recipe preparation retains one complete native baseline closure and remains a cost constraint; source-generator fan-out reduction is not a whole-pipeline speed/recipe claim. Keep this parent In Progress and MR !335 draft. No merge, deployment or Done.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The adopted integration maintenance contract and verification guidance accurately distinguish approved requirements from as-built evidence, and state exactly four TOTAL final flake check attributes
- [ ] #2 The coverage map assigns accepted obligations one primary owner/scenario, independent expected results, evidence and truthful status; distinguish executed coverage, required gaps, obsolete suites and lower-level verification retained outside checks
- [ ] #3 Final `nix eval .#checks.x86_64-linux --apply builtins.attrNames --json` returns exactly integration-access, integration-delivery, integration-assurance and integration-upgrade, with complete accepted coverage and no other attributes
- [ ] #4 Each normal native Snowfall owner is a pkgs.linkFarm aggregate of all explicitly registered scenario gates and propagates every failure; no placeholder owners
- [ ] #5 Shared helpers use lib/integration-test; coordinator composes owner-local compatible case fragments with the fewest justified scenario boundaries; no duplicate execution without distinct evidence
- [ ] #6 Delete the authorized nine legacy suites and obsolete manual jobs after relocating needed assets. Useful Rust/database/package/schema/doc/tool tests remain outside flake checks; consolidate duplicate selections or justify distinct conditions. Source retirement does not mark missing accepted obligations covered
- [ ] #7 No production/migration/SQLx/UI/design changes except separately authorized TASK-490.25/.26 and the subsequent owner-authorized TASK-495 offline-realization repair; no unrelated product fixes
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
## Verified code

Revision `bea0c29936c2220a8c1ba8dbfeb5750dacc18cde` passed pipeline https://gitlab.com/crystal-forge/crystal-forge/-/pipelines/2964502717. All eight required jobs succeeded: Access, Delivery, component, state-machine, CVE, dashboard, complexity and coverage. Archived results confirm Access 23/23 and Delivery 8/8 with that exact revision. Hosted database trace: 515 passed, 0 failed, 0 ignored; 227 Rust summaries and populated migration rehearsals.

Both normal owner commands also exited 0 locally at that clean source. Latest driver times: Access 54.18 s and Delivery 79.32 s. Delivery configures 10 GiB over five roles; peak memory unmeasured. One controller/database serves each independent lifecycle. Immutable preparation may be shared; live state never crosses scenarios.

TASK-495 product commit 6844d185 came from a separate worktree and was applied as fc0c5836. It is Review, not merged. No offline=false workaround or expected failure was used. TASK-496 heartbeat classification and P3/TASK-492 startup maintenance remain explicit unresolved variants. The initial .7 proof is verified; its later functional contribution criterion remains open.

## Organization and remaining scope

All six task-numbered Access fragments were renamed to semantic paths with IDs, receipts, assertions and ordering preserved. Delivery uses source-build.py inside one lifecycle and the same harness. No task-specific VM, root tests/ directory, generated coverage registry or new testing framework exists.

Current native checks are Access and Delivery only. The catalog records 79 rows: 11 with passed subsets, 59 planned and nine decision-blocked. Extended workflows and final four-owner/minimization acceptance remain unmet. Parent and coordinator tasks remain In Progress; MR !335 remains draft. Reporting-only notes record the verified code point and do not change executable assertions. No merge/deploy/Done.

## Historical metrics and limits

The pre-Delivery consolidation removed 15 verify outputs, reduced public packages 43 to 28, expanded jobs 14 to 9, and measured lower-level recipes 7,563 to 7,547. Those are not final-head whole-pipeline runtime/recipe claims. Source-generator direct dependencies later measured 2,525 to five, but raw archive transport still needs one complete native baseline recipe closure for cold-store validity. Cold preparation remains a minimization constraint. The superseded/canceled and failed cold-preparation pipelines remain historical evidence, not passes.

Detailed assertion ownership, the entire product inventory, source dispositions, exact commands and immutable evidence paths remain in the existing coverage map/catalog and TASK-490.24.
<!-- SECTION:NOTES:END -->

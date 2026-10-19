---
id: TASK-490
title: Replace integration checks with four contract-owned integration owners
status: In Progress
assignee:
  - '@openai'
created_date: '2026-10-17 16:13'
updated_date: '2026-10-19 15:46'
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
Build a small, trustworthy product-testing architecture with exactly four final native Snowfall owners: integration-access, integration-delivery, integration-assurance and integration-upgrade. Organize permanent files and scenarios by stable product behavior, never by tracking task or implementing agent. Minimize unnecessary VM lifecycles, duplicate assertions/builds and CI overhead while proving accepted CF contracts.

The nine legacy suites and fifteen verify-* wrappers remain retired. Unique protection runs in ordinary components, direct module/runtime/schema/tool CI and the existing isolated PostgreSQL workflow. There is no replacement aggregator, registry or fifth check. Current native checks are Access and Delivery; Assurance and Upgrade are absent, not placeholders.

Access retains its original23cases in one local-auth controller/database lifecycle. The six task-numbered fragments were renamed to sessions/authorization/admin-forms without changing case IDs, receipts, assertions or deliberate ordering. Delivery now has a real five-role lifecycle proving an authored Git commit, native evaluation and authoritative drv, an authenticated API-only builder's real offline build, independently cold nonce output, signed local publication, separate builder sessions and real target registration/periodic reporting. Eight Delivery cases passed locally. These subsets do not complete either owner.

The existing coverage map inventories79obligations across12families:11rows with passed subsets,59planned and9decision-blocked. Failed runtime variants remain explicit. Current implementation locates surfaces; accepted requirements and designs define expectations. No percentage or complete-row claim follows from a passing subset.

The corrective iteration repaired outer/nested truly offline Nix execution, declared absolute Bash launchers, root PostgreSQL privilege separation, short invocation-owned socket paths and XCCDF realization. Corrective hosted pipelines2961382083 and2961428492 passed all seven pre-Delivery required jobs. Full local isolated database execution passed515selected Rust cases and both populated migration rehearsals under outbound-denied client networking. Final clean-head Access/Delivery and hosted eight-job evidence still require recording.

The owner explicitly authorized the separate TASK-495 product fix after offline=true exposed an unsupported nix-store flag. It was developed in its own worktree and applied as a separate commit, with offline=true and all Delivery assertions retained. Prior .25/.26 repairs remain integrated. No other production/UI/API/schema/design repair, merge, deployment or Done is authorized. TASK-496 heartbeat classification remains Backlog.

Remaining scope includes Access environment/forms, authorization decisions, shell/preferences/notifications/OIDC; extended Delivery ingestion/strategies/scheduling/publication/activation/recovery; all Assurance; and the supported Upgrade transition. Keep behavior-oriented scenario boundaries and justify incompatible configurations or isolation, not tracking packets. Final acceptance requires all four owners and complete accepted coverage or explicit scope disposition, not merely a green partial pipeline.
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
## Current milestone

Coordinator openai resumed the existing TASK-490.1 worktree and draft MR !335. Access source organization is behavior-oriented and its23case IDs/receipts/order are preserved. The post-cleanup normal Access aggregate passed at590ee94c. Corrective hosted pipelines2961382083 and2961428492 passed all seven pre-Delivery required jobs, including full database execution and component/schema checks.

Initial Delivery is implemented, not merely planned: one native owner, one five-role lifecycle, eight mandatory cases. Latest local run passed all8 at a working tree based on fc0c5836; evidence `/nix/store/bys8qx6hix4kvf1yqycjbiqq2pmr1sc4-vm-test-run-cf-it-delivery-delivery-lifecycle-run`, driver100.17s, configured10GiB, peak unmeasured. It preserves exact Git/attempt/drv/job/session/target identities and proves the nonce output absent before real build and valid afterward. Immutable baseline dependencies are shared; live state never crosses scenarios.

TASK-495 was separately authorized, developed in its own worktree and committed6844d185; coordinator applicationfc0c5836. No offline=false workaround or xfail was used. TASK-496 remains a separate observed product defect. Existing P3/TASK-492 startup maintenance failure remains visible.

Final clean-revision and hosted Delivery/product-fix verification are pending recording. The parent, .2, .3, .7 and .24 remain In Progress; final four-owner acceptance is unmet. No merge/deploy/Done.

## Historical consolidation evidence

All15verify outputs were removed, packages43->28 and pre-Delivery expanded CIjobs14->9. Broad lower-level dependency union7563->7547 was measured before Delivery/TASK-495; no final-head recipe or overall speedup claim. Six ignored overlaps,94repeated normal cf-server selections and2duplicate integration invocations were removed. Core cf-config selection expanded13CA-only->94units; TASK-495 adds2ordinary regressions, so gross duplicate removal is not a net whole-pipeline test reduction.

At clean c254b1c5, Access23cases and the canonical isolated database path515tests/both populated migration rehearsals passed. Later offline/root-runner evidence is in TASK-490.24. Exact assertion ownership, all79obligations, historical source dispositions and remaining11partial/59planned/9blocked coverage are maintained in the existing coverage map and catalog. Old notes that Delivery roles were absent are superseded by this implementation.
<!-- SECTION:NOTES:END -->

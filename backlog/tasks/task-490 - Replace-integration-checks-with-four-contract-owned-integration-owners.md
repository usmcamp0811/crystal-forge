---
id: TASK-490
title: Replace integration checks with four contract-owned integration owners
status: In Progress
assignee:
  - claude-opus-5-5
created_date: '2026-10-17 16:13'
updated_date: '2026-10-18 18:36'
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
## Current source retirement (2026-10-18)

The owner's source-removal instruction supersedes all prior legacy-retention and optional-manual coexistence rules. Implementation 0105e512 removes nine suites and their manual jobs, relocates needed browser/XCCDF assets, and moves 17 lower-level definitions to native packages. Actual checks.x86_64-linux attrNames: ["integration-access"], down from27. The final criterion remains exactly FOUR TOTAL attributes, not four owners plus retained utility checks. Native Snowfall discovery unchanged; no placeholder owners.

Access's unchanged16-case aggregate passed at clean0105e512 with a fresh VM execution (24.23s driver); source evidence /nix/store/n68lcq6xxab1rhhfymp7yz4pj19hff95-vm-test-run-cf-it-access-lifecycle-local-bootstrap-run. No new cases this source-retirement pass. No product/UI/schema/design changes.

TASK-490.24 records exact verification results and two600second local command timeouts (full DB regressions and builder-packaging/XCCDF dependency compilation), so full verification is incomplete. Lightweight contracts, tooling, docs, OSCAL, module validation, CI routing/lint passed. Preserved DB regression definition/derivation is identical and actual isolated PostgreSQL execution was observed. Duplicate ignored Rust/DB selections between verify-server-regressions and state-machine-tests remain to map/consolidate here before final completion.

Child states: .1/.25/.26 Review; .2/.24 In Progress. Other coverage children remain Backlog, not silently selected. Continue accepted coverage through existing tasks, smallest same-scenario fragments and six planned scenario boundaries. Forms .3/.4, notifications .5, OIDC .6, roles .7, Delivery/Assurance/Upgrade are not implemented merely because old suites were deleted. Environment-scope/registration and Upgrade decisions remain scoped to their own cases. Final four-owner inventory/coverage not satisfied. Existing MR !335 stays draft; no merge/deploy/Done.

## Historical milestones

Foundation organization started at683c86e0; correct Access startup assertion exposed .25/.26. Authorized prerequisite repairs52128b3a and76e51464 enabled bootstrap; sessions passed81d65be7 and CI-path2c494ac4. Selected Operator/Viewer cases extended the same scenario to16 at8a857bc8; hosted job passed08a79037. Interim early-CI manual-coexistence policy and source-retention rules applied at those revisions only, and are now superseded. Historical records/receipts remain in the coverage map and child tasks. TASK-492/493/494 are unrelated recorded issues, not authorization for product fixes.
<!-- SECTION:NOTES:END -->

---
id: TASK-499
title: Restore certified rollback authority for native-evaluated deployments
status: In Progress
assignee:
  - openai
created_date: '2026-10-20 04:21'
updated_date: '2026-10-20 22:47'
labels: []
dependencies: []
references:
  - packages/default/crates/cf-server/src/models/primary_evaluation.nix
  - packages/default/crates/cf-server/src/services/composite_enforcement.rs
  - packages/default/crates/cf-server/src/queries/evaluation_snapshots.rs
  - packages/default/crates/cf-server/src/queries/systems.rs
  - docs/knowledge/evaluation/evaluation-snapshot-retention-and-rollback.md
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/335'
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/jobs/17305508230'
modified_files:
  - docs/knowledge/evaluation/evaluation-flake-snapshot-architecture.md
  - >-
    docs/knowledge/evaluation/evaluation-snapshot-persistence-bounds-and-redaction.md
  - docs/knowledge/evaluation/evaluation-snapshot-retention-and-rollback.md
  - >-
    packages/default/crates/cf-server/migrations/0310_native_deployment_identity.sql
  - packages/default/crates/cf-server/src/deployment/mod.rs
  - packages/default/crates/cf-server/src/handlers/api/systems.rs
  - packages/default/crates/cf-server/src/models/evaluate_with_policies.rs
  - packages/default/crates/cf-server/src/models/evaluation_snapshots.rs
  - packages/default/crates/cf-server/src/queries/commits.rs
  - >-
    packages/default/crates/cf-server/src/queries/commits/partial_resource_failure_tests.rs
  - packages/default/crates/cf-server/src/queries/evaluation_snapshots.rs
  - packages/default/crates/cf-server/src/queries/scanning.rs
  - packages/default/crates/cf-server/src/queries/system_states.rs
  - packages/default/crates/cf-server/src/queries/systems.rs
  - packages/default/crates/cf-server/src/services/composite_enforcement.rs
priority: high
type: bug
ordinal: 550000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Restore certified retained-generation rollback authority for real CF-managed native deployments. At df11c800 two actual UI-requested deployments and changed-state heartbeat correlation succeeded, but both deployment snapshot bindings were null and the first generation remained rollback-ineligible. Native PRIMARY produces exact build identity but no available V1 Config inventory; Inspector V2 has an independent selector and cannot be assumed to authorize retained rollback. The owner now explicitly authorizes the smallest coherent producer/binding/retention repair while preserving exact immutable lineage, old artifact semantics, read-only historical reads and bounded fast evaluation. A forward migration is allowed only for the genuinely required typed artifact representation; no historical recertification or authority seeding.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The accepted native deployment workflow obtains certified rollback authority with exact system/configuration/commit/derivation/output identity.
- [ ] #2 Two actual CF-managed native deployments permit authorized retained-generation UI rollback and actual target restoration without seeded authority.
- [ ] #3 Artifact schema versions and selector/retention boundaries remain explicit and are covered by regressions and updated source documentation.
- [ ] #4 Missing or mismatched certified lineage remains ineligible and cannot be bypassed by a supplied store path.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Dedicated worktree/branch TASK-499-retained-authority, clean base df11c800; Sol retains integration/lock and will establish task-owned preview before browser-visible implementation. User explicitly authorizes smallest repair and necessary forward migration.
Producer/binding/retention choice: create a separately typed bounded native deployment-identity manifest, proposed schema3/integrity3, through existing immutable evaluation-artifact infrastructure. This is NOT V1 Config inventory or V2 Inspector evidence. It records actual selected native attempt, full commit/configuration, exact persisted derivation row/.drv/output identities without any extra Nix traversal. PRIMARY selector explicitly supports V1 plus the native deployment type; Config selector remains V2-only. Ordinary deployment captures that exact certified identity; observation retention and rollback use immutable deployment/retained IDs, never mutable latest. Deployment-only Config inventory reads remain unavailable, not empty/complete. Historical V1/V2 meanings and rows are unchanged.
1. Verify schema3 is unused and add the next forward migration only because existing schema/certification constraints and immutable selector/retention triggers cannot represent this distinct type. Validate fixed bounded shape/exact lineage and freeze manifest content; no blanket recertification or synthesis from old mutable rows. Preserve retention/deletion/24h/batch bounds and lock order snapshot writer -> commit/attempt or existing flake/system locks.
2. Publish inside successful native per-configuration persistence after exact active owner/inventory and actual derivation checks. Mock/stale/failed/excluded/path-conflicting/incomplete results mint no authority. Keep finalizers from overwriting successful native manifest merely because optional Config inventory is absent.
3. Update each deployment binding, reciprocal/ordinary retention, generation eligibility and rollback resolver with explicit version-aware identity validation, not accepting another integrity number generically. Preserve V2 exclusion and legacy supported V1. Fix only any demonstrated newest-request fallback defect needed to preserve existing accepted negative guarantee.
4. Focused Rust/isolated DB regressions: real producer -> binding -> observation retention -> eligible rollback; wrong system; same output/different commit/derivation; stale/failed/uncertified/mismatched evidence; immutable retained identity after selector advance; Config reads unavailable/no mutation; migration old semantics and bounded reclamation. Update affected contracts/specs and SQLx metadata only when shapes require it.
5. Commit independently and integrate into existing Delivery owner. Keep mandatory recovery assertion unchanged; run A -> B -> History rollback -> actual restoration -> state/twoH/reload. No new owner/scenario/package for this repair.

Owner review follow-up on d68 (integrated by Sol as 62e86efa): reproduce old-unbound native recertification and newest pending/succeeded incomplete-lineage fallback. Preserve applied0310. Use conservative native publication-before-request/observation ordering, with actual publication clock after writer lock; preserve V1 late-publication semantics. Rank newest same-path request before complete immutable tuple validation in repair/external fallback and reciprocal observations. Bound reciprocal publication work newest-first to existing 100-row maintenance convention. Strengthen populated certified V1/V2 upgrade and native terminal24h release tests. Focused isolated PostgreSQL tests only, no VM/preview DB changes; separate follow-up commit, no amend. Assess forward migration only if existing schema cannot enforce the chosen contract.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Verified baseline: df11c8005de5ab7088015757dafc7fa7602da188; hosted pipeline2966113736 Deliveryjob17305508230 failed mandatory rollback eligibility after13passingcases. Access23 and database517/0/0 passed; component/CVE/dashboard passed; complexity/coverage skipped. Downloaded /tmp/opencode/hosted-delivery-df11c800.json confirms fresh generation1 is not current after B, retained UUID null, eligibility false. The current owner assignment authorizes implementation. Prior nonauthorization wording is obsolete and removed. Research identifies a missing bounded native deployment-authority producer; do not solve by accepting V2 as V1 or restoring an eager full Config crawl.

Repair implementation in TASK-499-retained-authority at df11c800: added forward 0310 for bounded schema3/integrity3 Native deployment identity, active producer certification and shared exact-live-lineage predicate; PRIMARY supports legacy V1 and certified3, Config remains V2-only. Existing binding/retention/rollback/read consumers are being updated; no historical recertification or artifact->derivation FK. Test database launcher /tmp/opencode/task499-db.sh uses fresh invocation-owned PostgreSQL 35437 and repository database-environment.sh ownership/preload checks. Baseline focused legacy deployment/finalization test passed (exit0), logs /tmp/opencode/task499-negative-before2.log. Stronger wrapper-negative and producer-chain tests in progress. Preview http://127.0.0.1:8082/systems verified HTTP200; still baseline backend, owner will refresh after build. Preview/user databases untouched. SQLx stored-query inventory inspection found no checked metadata referencing affected artifact tables or changed authority views; new SQL uses runtime queries.

Focused repair evidence: initial native producer -> immutable deployment binding -> observed retention -> eligible rollback and malformed/exact-live-lineage tests passed (2 tests; /tmp/opencode/task499-focused-before.log, exit0). Added dedicated legacy-V1 ingestion wrapper negative before repairing fallback: it failed with exit101, 'ingestion fell back through newer failed request' (/tmp/opencode/task499-wrapper-before2.log and .status). Fixed ranking before identity/status checks in ordinary retention, blocked reciprocal binding through newer same-path requests, and blocked newest failed/superseded/outside-24h fallback in ingestion/current repair. Expanded native tests include actual normal finalizer preservation, V2 exclusion, Config Changed-page unavailable/no mutation, failed/uncertified certification refusal and GC; partial-resource finalizer regression added in existing test module. An expanded test compile found and corrected a test-only owned/borrowed V2 argument mismatch; verification rerunning. Documentation validation via controlled public-cache-build.sh .#okf-knowledge passed exit0 (/tmp/opencode/task499-docs.log). Offline Cargo server binary build passed on an intermediate repair revision; final backend freshness still pending.

Independent repair committed d68b3c44ebc9296529bfae7eb52ed519181b239e on TASK-499-retained-authority, parent df11c800; worktree clean after commit. No push/MR/task completion. Final focused DB run /tmp/opencode/task499-final-tests4.log + .status exited0: 4 native identity/upgrade tests, 1 wrapper negative, 8 partial-resource tests, 6 native-outcome tests, 4 legacy binding/retention/GC compatibility tests =23. Existing poam_workflows external_ (5) and fleet_cve_inventory_separates_current_scheduled_and_historical_scope (1) passed, runner exit0: /tmp/opencode/task499-cve-tests2.log + .status. Fresh invocation-owned PostgreSQL 35437/35438 with repository database-environment.sh safety checks; preview DB untouched. Native negative fixtures were corrected to respect immutable carrier FKs and global single-active-evaluation ownership, without changing production derivation/ownership rules. Initial CVE test selection mistakenly selected only ignored tests (zero executed); corrected run uses --include-ignored and executed all six. Final targeted rustfmt --check and git diff --check passed. Controlled public-cache documentation validation passed exit0. SQLx metadata unchanged: no checked-query shapes reference the additive artifact column or changed authority views; runtime SQL only. Offline Cargo server build passed (binary packages/default/target/database-tests/debug/server; /tmp/opencode/task499-backend-final.log/.status). Final VM-free offline Nix core package build is still running in tmux cf-task499-package-final, /tmp/opencode/task499-package-final.log/.status. Earlier Nix builds were canceled when test source changed; no success claimed for canceled runs. Sol owns backend refresh and unchanged A->B->History rollback->actual restored state/twoH integration. Last verified preview http://127.0.0.1:8082/systems remains baseline backend until owner refresh is verified; no VM execution claimed.

Final VM-free offline Nix package build passed exit0: /tmp/opencode/task499-package-final.log and .status. Server library 1828 passed/775 ignored; cf-config 96 passed; normal binary/native post-checks passed. The separate 29 focused database tests were actually executed as recorded above; package-ignored DB tests are not claimed executed. Derivation /nix/store/dvgj2092rqsvi718fby41q83yn82gdv4-cf-server-core-0.3.0.drv matches offline evaluation from committed d68b3c44. Ready backend wrapper: /nix/store/r9q2xq2kciy399kh90sfyxa9g4wykv6r-cf-server-core-0.3.0/bin/server; native executable .server-wrapped in same directory (store listing verified both). Complete exact commands, exits, test selections, migration/metadata disposition and evidence paths: /tmp/opencode/task499-verification.md. HTTP200 preview availability rechecked at http://127.0.0.1:8082/systems; last verified backend baseline until Sol refreshes. No preview/shared DB migration, reseed, or test use by worker. Commit/worktree remain clean; task stays In Progress for Sol integration and unchanged mandatory actual-restoration/twoH proof.

Follow-up regressions reproduced on d68: exact filter queries::evaluation_snapshots::tests::review_ ran 2 tests, both failed (exit101); /tmp/opencode/task499-review-before-exact.log/.status. Older unbound request acquired later snapshot UUID; newest pending B missing lineage fell back to A. Follow-up uses existing immutable created_at: schema3 producer explicitly stamps clock_timestamp() after writer lock, native binding/retention requires certificate creation <= immutable request issuance <= observation. Replay returns the existing selected UUID without rebinding old requests. Current-observation repair reuses ordinary newest-request full-tuple validation, and external reconciliation excludes observations with any same-path CF request issued beforehand. Native reciprocal selection capped newest-first100 requests, each reciprocal statement caps100 missing generations prioritized by latest observation time (including rollback to lower profile generation); already retained rows skipped. No forward migration is needed for these query/constructor checks on existing immutable fields; applied0310 remains untouched, with no historical recertification. Strengthened upgrade fixture now includes populated certified V1 and V2 content/reference preservation. Added native bounded backfill/latest-ingestion/24h terminal-release test and full-wrapper pending/succeeded missing/mismatched tests with sealed schema1 scan to exercise external fallback. Focused verification running; final commit pending.

Owner-review follow-up committed separately as f7751ad28815a49c66c8a295f9d8da9856467d8d (parent d68b3c44; Sol parent integration 62e86efa). Six files only; clean worktree; no amend/push/MR/Done. Final source verification: 26 library/database cases exit0 in /tmp/opencode/task499-review-complete.log/.status and 6 existing poam_workflows authority-read cases exit0 in /tmp/opencode/task499-review-complete-cve.log/.status. Final three review regressions rerun exit0 including equal-timestamp lower-generation rollback priority, old-unbound/native-later-attempt refusal and full ingestion wrapper missing/mismatched B refusal with external-scan evidence. Populated certified V1/V2 upgrade and native terminal24h release passed. Fresh offline Cargo backend build exit0 (/tmp/opencode/task499-review-final-backend.log/.status), targeted rustfmt/Git diff checks exit0, controlled public-cache knowledge validation exit0 (/tmp/opencode/task499-review-docs.log/.status). Existing migration0310 and all migration files exactly match d68; no forward migration or SQLx metadata change needed. Full exact commands, selections, bounds, temporal proof, migration disposition and evidence: /tmp/opencode/task499-review-verification.md. No full Nix package/VM repeat for this focused follow-up. Preview HTTP200 at http://127.0.0.1:8082/systems; preview backend still old until owner refresh. No preview/shared DB changes. Sol should cherry-pick only f7751ad2 and retains actual restored-state/twoH integration ownership.
<!-- SECTION:NOTES:END -->

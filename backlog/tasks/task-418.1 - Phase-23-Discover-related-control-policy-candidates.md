---
id: TASK-418.1
title: 'Phase 23: Discover related-control policy candidates'
status: In Progress
assignee:
  - '@agent'
created_date: '2026-08-12 19:26'
updated_date: '2026-08-12 22:36'
labels: []
milestone: m-23
dependencies:
  - TASK-418
references:
  - c586de2e2b445bf8d2429046fc5b209b0c9f991b
modified_files:
  - packages/default/crates/cf-server/src/queries/framework_requirements.rs
  - packages/default/crates/cf-server/src/queries/compliance_interchange.rs
  - packages/default/crates/cf-server/src/compliance/requirement_model.rs
  - packages/default/crates/cf-server/src/compliance/xccdf/import_models.rs
  - packages/default/crates/cf-server/src/compliance/xccdf/importer.rs
parent_task_id: TASK-418
priority: high
type: enhancement
ordinal: 413000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Add candidate-only discovery for DISA STIG requirements that share exact normalized CCI or SRG identifiers with requirements from trusted mappings on current accepted policy versions. Related candidates must remain review-only, must not become automatic MapExisting proofs, and must not make imports auto-resolvable by themselves. Preserve existing authoritative, inherited, and exact technical ordering and behavior. Do not add fuzzy matching, title similarity, embeddings, LLM matching, or requirement crosswalk tables.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Related candidate discovery uses normalized requirement metadata and exact normalized CCI/SRG identifier equality without introducing a second STIG identifier store.
- [ ] #2 Candidate discovery considers only trusted mappings on current accepted policy versions and excludes untrusted mappings, nonaccepted versions, and accepted versions that are not current-published.
- [ ] #3 RelatedMapping candidates include shared CCI or SRG identifiers and enough existing requirement evidence for review presentation, with confidence below deterministic mapping and exact technical candidates.
- [ ] #4 Candidate ordering remains AuthoritativeMapping, InheritedMapping, ExactTechnicalMatch, then RelatedMapping, with deterministic candidates winning when the same policy is found through multiple paths.
- [ ] #5 RelatedMapping candidates never make preview auto_resolvable true by themselves and never become MapExistingProof values.
- [ ] #6 A reviewed related-candidate selection is represented distinctly from deterministic technical proof and does not claim exact equivalence.
- [ ] #7 Unit tests cover same CCI, same SRG, no shared IDs, substring mismatch, untrusted mapping, nonaccepted mapping, stale accepted mapping, and deterministic candidate precedence.
- [ ] #8 Database-backed tests verify normalized CCI/SRG relationships produce RelatedMapping candidates and negative trust/currentness cases do not.
- [ ] #9 Targeted candidate and Phase 22 regression tests pass; cargo fmt --all --check, git diff --check, and cargo check -p cf-server pass.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
## Phase 23 implementation plan

1. Inspect the existing requirement candidate DTOs, normalized metadata representation, preview auto-resolution calculation, and import proof validation without changing behavior.
2. Add a typed CCI/SRG identifier extraction and normalization helper backed by requirement metadata.
3. Extend normalized candidate discovery with trusted/current accepted mappings for exact shared CCI/SRG evidence, preserving deterministic candidate precedence and review-only semantics.
4. Adjust preview auto-resolvable calculation to recognize only permitted deterministic proof classes and inferred enforcement, never RelatedMapping alone.
5. Add the smallest explicit reviewed-related-selection representation needed for commit semantics; preserve the existing restriction that RelatedMapping cannot become MapExistingProof.
6. Add unit tests for identifier matching and precedence, plus DB-backed tests proving trust/currentness filtering against normalized rows.
7. Run targeted candidate and Phase 22 regression tests, cargo fmt --all --check, git diff --check, and cargo check -p cf-server. Avoid the long web-ui check unless a changed UI surface makes it necessary.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
2026-08-12 Phase 23 implementation in existing TASK-418 worktree: added typed RelatedRequirementIdentifiers metadata extraction with uppercase exact normalization; extended find_policy_candidates to discover trusted mappings on current accepted/current-published policy versions from other frameworks and emit RelatedMapping confidence 70 with shared identifier and existing requirement evidence; changed auto_resolvable to only accept authoritative/inherited/exact technical candidates or inferred enforcement; preserved RelatedMapping exclusion from MapExistingProof. Added unit tests for metadata normalization and review-only auto-resolution, plus an ignored DB test proving normalized related mapping discovery and trust/currentness filtering. Targeted DB test passes; cargo fmt and git diff checks pass; SQLX_OFFLINE cargo check passes with existing warnings. Work remains uncommitted.

Phase 23 candidate-discovery slice committed and pushed as f582fdea50ebc2e0ae4235c2a98001da02efbde3. Remote branch now points to this commit.

Follow-up committed and pushed as b388c3f6d0d384c159fe13f9f1413984bf5c9f9a: added ReviewedRelatedCandidate provenance to ImportedMappingSemantics and persisted selected related reviews as suggested provenance without treating them as MapExistingProof. cargo fmt, git diff --check, SQLX_OFFLINE cargo check -p cf-server, and targeted unit test passed.

Reviewer-directed follow-up completed in pushed slices: 9d9a4d10 exposes structured RelatedCandidateEvidence and excludes RelatedMapping from shared automatic candidate intersection; af773f39 adds reviewed-related action validation, commit-time current/trusted/exact identifier revalidation, and suggested provenance precedence; 723be541 adds regression coverage for related-only shared groups. SQLX_OFFLINE cargo check and focused unit test passed. No web-ui check run.

Verification update: related candidate DB test passed; map_existing_stig regression group passed 3/3. Phase 22 shared-creation group ran 7 tests with 6 passing; exact reimport idempotency test failed due shared test-database deployment_policies_name_key collision during fixture setup, not an implementation assertion. Worktree clean and remote branch remains at 723be5415a07efda8c3033e38939f8e14cfa5674.

Latest Phase 22 run: 6/7 passed. Rollback collision test now passes; exact reimport idempotency still fails with counts increasing by (1 framework version, 1 bundle version, 3 requirement versions, 3 mappings), indicating fixture/database state or exact-artifact identity behavior remains unresolved. Worktree is clean at pushed commit cc6aadc7.

Diagnosed exact reimport as test interference: the test passed alone; serialized Phase 22 suite passed 8/8. Replaced global-count idempotency assertions with source-artifact/bundle-scoped counts and explicitly asserted the benchmark source-object mapping fast-path prerequisite. Removed the unused reviewed-revalidation record parameter. Changes pushed as 195e0fc4.

2026-08-12 follow-up pushed as 53848d1f: added ignored DB-backed reviewed-related commit tests for successful cross-framework reuse with suggested provenance and forged evidence rollback, plus a metadata-bearing STIG fixture. Verified with nix develop cargo fmt --all --check, git diff --check, cargo check -p cf-server, targeted related unit tests (7 passed, 3 ignored), the existing related-candidate DB test (1 passed), and both reviewed-related DB tests (2 passed). The Phase 22 wildcard invocation was not useful because Cargo does not support wildcard test filters; prior serialized Phase 22 suite had passed 8/8. Remaining: broader candidate negative/precedence DB coverage and final serialized Phase 21/22/23 regression/build gate.
<!-- SECTION:NOTES:END -->

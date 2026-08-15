---
id: TASK-418
title: >-
  Implement Cross-Framework Reusable Policies, Compliance Requirements, and
  Requirement-Aware STIG Import
status: In Progress
assignee:
  - '@agent'
created_date: '2026-08-11 17:37'
updated_date: '2026-08-15 19:29'
labels: []
milestone: m-22
dependencies:
  - TASK-412
references:
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/315'
priority: high
type: enhancement
ordinal: 412000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Implement the normalized compliance requirement and policy mapping architecture that follows MR !313.

Policies must become framework-neutral reusable technical implementations.

Frameworks define requirements. Policies map to requirements. Compliance bundles select exact policy versions and define a requirement baseline. Imported STIG content must reconcile requirements and existing policy implementations before creating new policies.

The production UI for every area touched by this task must match the design example from commit:

```text
861fd877
MC ◯ update ui design for policy mapping
```

The design example is the visual and interaction source of truth.

Relevant design files include:

```text
docs/design/CrystalForge/data-mappings.js
docs/design/CrystalForge/components/PoliciesView.jsx
docs/design/CrystalForge/components/ComplianceView.jsx
docs/design/CrystalForge/components/ImportStigModal.jsx
docs/design/CrystalForge/crystal-forge.html
```

The production Dioxus UI must reproduce the relevant design states pixel-for-pixel, including spacing, typography, borders, tabs, chips, colors, empty states, interaction states, modal dimensions, grouping, and information hierarchy.

Do not copy mock-only architecture or legacy shortcuts from the design example where they conflict with this task's backend model.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Policies are framework-neutral in the authoritative backend model — a policy may map to zero, one, or many requirements across multiple frameworks
- [ ] #2 Normalized compliance_frameworks and compliance_framework_versions tables exist with uniqueness constraints and semantic digests; duplicate authoritative release identity returns a typed conflict rather than a silent duplicate
- [ ] #3 Normalized compliance_requirements (lineages) and compliance_requirement_versions tables exist; a requirement appearing in multiple framework releases retains one lineage with separate immutable versions
- [ ] #4 policy_requirement_mappings is a first-class many-to-many join between exact policy versions and requirement versions, supporting relationship (implements/supports/provides_evidence_for), coverage (full/partial), rationale, and provenance (manual/imported/inherited/inferred)
- [ ] #5 Mappings on an accepted/published policy version are read-only; editing requires creating a derived draft per the !313 derived-draft workflow
- [ ] #6 compliance_bundle_version_requirements provides explicit requirement membership for bundle versions, separate from policy membership
- [ ] #7 Backend derives requirement coverage (full/partial/unmapped) from normalized mappings + bundle requirement membership + selected bundle policy versions; legacy policy.framework/control_family fields are not authoritative
- [ ] #8 A DISA STIG import first creates/reconciles framework and requirement state before making policy decisions; policies are the secondary implementation step
- [ ] #9 STIG import preview classifies each requirement as EXISTING_UNCHANGED / EXISTING_CHANGED / NEW_REQUIREMENT / REMOVED_FROM_RELEASE / IDENTITY_CONFLICT and proposes ordered policy candidates (authoritative mapping → inherited → exact technical match → related mapping → fuzzy suggestion → none)
- [ ] #10 Atomic STIG commit re-validates artifact digest, re-parses bytes, re-computes all identities, acquires advisory locks, and rolls back completely on any failure (TOCTOU-safe, matching !313 guarantees)
- [ ] #11 Exact re-import of the same artifact is fully idempotent — zero duplicate framework versions, requirement versions, policies, mappings, or bundle versions
- [ ] #12 New framework release import reuses framework lineage and requirement lineages, inherits unchanged mappings, flags changed requirements for review, and only creates genuinely new policies
- [ ] #13 Policy-to-requirement mapping CRUD APIs exist for mutable draft policy versions; read APIs for requirements, framework hierarchy, and bundle coverage are server-side with pagination
- [ ] #14 Requirement search is server-side and scoped by framework/version, supporting external ID, title, CCI, and SRG
- [ ] #15 Policy UI implements the mapping workflow matching commit 861fd877 pixel-for-pixel: policy cards, drawer, add/edit modal with Details/Mappings/Enforcement/Evidence tabs, inline mapping editor, framework selector, server-backed requirement search, requirement hierarchy breadcrumb, mapping display grouped by framework
- [ ] #16 Compliance view implements the Requirement coverage card from commit 861fd877 with full/partial/unmapped counts backed by authoritative server data, not frontend calculation
- [ ] #17 Bundle add/edit UI splits policy selection into 'Mapped to <framework>' and 'Custom addition / No mapping to <framework>' sections, matching the design pixel-for-pixel
- [ ] #18 STIG import UI implements the reconciliation summary step before per-control refinement; normal path auto-resolves most controls and surfaces only those requiring attention; 'Refine all' escape hatch preserved
- [ ] #19 Concurrent imports do not create duplicate framework lineages, requirement lineages, or mappings; concurrency tests cover identity races
- [ ] #20 Legacy compliance metadata fields (framework, control_family, cci_ids, srg_ids, etc.) remain preserved as source/advanced metadata but are not presented as authoritative compliance ownership in any UI surface
- [ ] #21 All required automated tests pass: framework CRUD/identity, release uniqueness, requirement lineage/hierarchy, mapping create/update/delete on draft, mapping blocked on accepted version, bundle coverage full/partial/unmapped, exact STIG re-import idempotency, new release reconciliation, inherited mapping, exact technical candidate, concurrent identity race, complete rollback on failure
- [ ] #22 nix build .#web-ui passes; nix build .#server passes; nix flake check --keep-going passes; no println!/dbg!/eprintln! in production paths; cargo fmt --all --check passes; git diff --check passes
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Review remediation plan (2026-08-15):
1. Narrow migration 0215 immutable backfill exceptions so policy backfill can change only pending mapping_digest and bundle backfill can change only pending requirement_digest; leave semantic digest and digest metadata immutable.
2. Add database immutability guards for compliance_framework_versions and compliance_requirement_versions. Permit only pending semantic digest finalization and the fixture/import hierarchy parent-link construction update; reject all other UPDATE/DELETE operations.
3. Make framework semantic identity include the ordered set of requirement semantic digests, and compute/check the final release digest after requirement versions are reconciled so same-release changed requirement content conflicts.
4. Fix mapping API nested-resource scoping, make manual mapping provenance server-authored as manual, allow operator/admin mapping CRUD, and filter bundle mapped-policy projections to trusted mappings.
5. Add focused regression tests for trigger/API/query behavior where existing test infrastructure supports it.
6. Verify with cargo fmt, SQLX_OFFLINE cargo check, targeted unit/DB tests if isolated PostgreSQL is available, and git diff check; commit and push each checkpoint.

Follow-up review remediation (2026-08-15):
7. Add one authoritative parsed-framework requirement canonical/digest collector and use it in preview plus commit; policy selection must not influence framework identity.
8. Add same-release reuse/conflict tests for artifact changes, changed requirements, and different policy selections.
9. Rework requirement hierarchy construction so parent links are assigned while rows are pending, then finalize requirement digests; remove the permanent finalized-row reparent exception and add a finalized reparent rejection test.
10. Add migration/backfill compatibility for framework digests produced before the requirement-aware canonical representation, with an upgrade-path test on isolated PostgreSQL.

Final review remediation (2026-08-15):
11. Make framework digest backfill lock-safe and conditional: recheck pending state under lock, skip rows finalized by another instance, and verify the resulting canonical digest.
12. Replace the vacuous backfill test with an explicit legacy row + requirement fixture and concurrent backfill calls asserting cf-model-json-2 digest and idempotent reimport.
13. Preserve release-specific DISA XCCDF Rule IDs in requirement_version.external_id while keeping stable V-IDs as canonical_requirement_key, with regression coverage.
14. Persist production DISA Group→Rule hierarchy using pending requirement construction before digest finalization; include a deterministic hierarchy projection in framework release identity or document and test the chosen semantics.

v4 upgrade hardening (2026-08-15): classify legacy DISA rows from persisted publisher rather than source-key spelling; fail closed for missing, corrupt, or mismatched artifacts before reconstruction. Rebuild every parsed group/rule requirement and verify persisted node/edge topology before finalizing the framework digest. Add a follow-up migration (do not edit already-applied 0220) to narrow its pending-framework trigger exception to the payload fields required for reconstruction. Replace the synthetic pending-row test with a real cf-model-json-3-to-v4 transition using identify_framework(), add negative DB coverage, seed the isolated database, then run all framework DB tests and required Nix builds/checks.

v4 metadata authority remediation (2026-08-15): make parsed DISA identity authoritative for every framework digest field during legacy reconstruction; reconcile persisted framework metadata through a new narrowly scoped migration path and keep digest state consistent with stored rows. Ensure framework lineage publisher classification is updated by authoritative adapter imports so missing artifacts cannot bypass fail-closed recovery. Add a real v3-to-v4-to-identical-reimport DB test plus negative artifact/classification cases, then restore a seeded isolated DB suite and complete feasible Nix verification.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
## Progress log

**2026-08-11**

### Phase A — Migrations (complete)
- `0211_compliance_frameworks.sql`: `compliance_frameworks` + `compliance_framework_versions` tables with uniqueness constraints and digest sentinel
- `0212_compliance_requirements.sql`: `compliance_requirements` + `compliance_requirement_versions` with GIN + FTS indexes
- `0213_policy_requirement_mappings.sql`: `policy_requirement_mappings` (with immutability trigger) + `compliance_bundle_version_requirements` (with immutability trigger)
- All three migrations applied against dev DB and verified (3 tables confirmed in DB)

### Phase B — Rust domain models (complete)
- `src/compliance/framework_model.rs`: `FrameworkVersionCanonical`, `write_framework_version_digest`
- `src/compliance/requirement_model.rs`: `RequirementVersionCanonical`, `write_requirement_version_digest`, reconciliation enums (`RequirementReconciliationState`, `FrameworkReconciliationState`, `PolicyCandidateMatchType`), DTOs
- Registered in `src/compliance/mod.rs`

### Phase C — DISA STIG adapter (complete)
- `src/compliance/xccdf/disa_stig_adapter.rs`: `is_disa_stig`, `identify_framework`, `canonical_key_for_rule`, `requirement_metadata`, `canonical_for_rule`, `hierarchy_nodes_for_rule`
- 7 unit tests: all pass

### Phase D — Query layer (complete)
- `src/queries/framework_requirements.rs`: 1468 lines
  - `list_frameworks`, `list_framework_versions`, `search_requirements`, `list_requirement_children`
  - `list_policy_mappings`, `create_policy_mapping`, `update_policy_mapping`, `delete_policy_mapping`
  - `compute_bundle_requirement_coverage`
  - `upsert_framework_lineage`, `insert_framework_version`, `upsert_requirement_lineage`, `insert_requirement_version`, `insert_bundle_version_requirement`, `insert_policy_mapping_in_tx`
  - `preview_framework_reconciliation`, `preview_requirement_reconciliation`, `find_policy_candidates`
- 5 DB-gated tests: all pass
  - `framework_lineage_is_idempotent` ✅
  - `framework_version_release_conflict` ✅ (returns FRAMEWORK_RELEASE_CONFLICT)
  - `requirement_lineage_is_idempotent` ✅
  - `mapping_blocked_on_accepted_policy_version` ✅ (returns POLICY_MAPPING_IMMUTABLE)
  - `bundle_coverage_full_partial_unmapped` ✅
- SQLx offline metadata updated and committed

### Phase E — API handlers (complete)
- `src/handlers/api/framework_requirements.rs`: all 9 route handlers
- Registered in `mod.rs` and `bin/server.rs`
- `SQLX_OFFLINE=true cargo check` passes

### Remaining: Phase F (Web UI), Phase G (more tests), Phase H (full verification + nix build)

**Commits so far:**
- `ab9e44f9` feat(compliance): add framework/requirement schema, domain models, and DISA STIG adapter
- `a5e552ed` feat(compliance): add framework/requirement query layer and DB-gated tests
- (staged: handlers + server routes — commit pending)

Phase F (Web UI) complete. Added API models and client functions for frameworks, requirements, mappings, and coverage. Policy editor modal now has a Mappings tab with grouped display, inline editor (framework/version/requirement search/relationship/coverage), and server-backed CRUD. Compliance view has a RequirementCoverageCard with full/partial/unmapped chips and expandable rows. nix build .#web-ui and .#server both pass. cargo fmt --all --check passes. 1066 lib tests + 5 framework_requirements DB tests + 14 compliance_interchange DB tests all green. Commits: ab9e44f9 (schema+models+adapter), a5e552ed (query layer+DB tests), a2f2dfbd (API handlers), 3a78bf28 (web UI), eada567a (fmt). Pushed to origin/TASK-418-cross-framework-requirements.

Resumed with user confirmation that no completion claim is valid until Policies and Compliance views have pixel-level parity with the design using real backend behavior. A read-only audit confirmed the STIG reconciliation path and mapped/custom bundle selection remain unimplemented.

Implemented an in-progress requirement-aware DISA STIG path: foreign preview now returns server-computed framework/requirement/candidate reconciliation data; the modal renders the design-aligned reconciliation summary, attention-only path, and Refine all path; exact artifact commit now returns the prior bundle result rather than creating duplicates; DISA commits persist normalized framework, requirement, bundle-baseline, and mapping rows in the existing transaction. `SQLX_OFFLINE=true nix develop ../.. --command cargo check -p cf-server` and `nix develop ../.. --command cargo check` from `packages/web-ui` passed (existing warnings remain). Further work is still required for release-change/inherited mapping semantics, bundle mapped/custom selection, full Policies parity, and browser-level pixel verification.

Committed and pushed `cbd8c72d feat(compliance): reconcile STIG imports and split bundle policies`. It includes the server-backed bulk framework mapping projection and design-aligned mapped/custom sections in both bundle add and edit flows. `cargo check` passed for server and web UI; warnings are pre-existing repository-wide warnings.

Implemented release-diff preview work: adapter-derived requirement semantic digests now classify incoming requirements as unchanged, changed, or new against the preceding release; prior-only requirements are emitted as removed; candidate lookup differentiates exact-version authoritative mappings from inherited mappings. Added a DB-gated changed/removed classification test. Verified with the targeted ignored DB test and server/web `cargo check`; existing repository warnings remain.

Implemented and pushed immutable STIG policy reuse: `MapExisting` is revalidated against a trusted mapping for the unchanged prior requirement, accepted selected policies are converted to the existing mutable derived-draft workflow, and the effective draft is used consistently for membership, mappings, and source-object provenance. Reuse candidates are now restricted to current accepted versions, preventing a deprecated/non-current implementation from being silently substituted. `SQLX_OFFLINE=true nix develop ../.. --command cargo check -p cf-server`, targeted rustfmt checks, and `git diff --check` passed; DB integration coverage for this commit-time path remains to be added. Commits: `6ebaf6ca`, `e22332d8`.

2026-08-11 MapExisting DB-proof slice: added three ignored DB-gated STIG commit-path tests in `packages/default/crates/cf-server/src/queries/compliance_interchange.rs`. They prove current accepted trusted inherited reuse, supports/partial/rationale preservation, shared draft reuse/membership deduplication/order/accounting, mutable/suggested/superseded/deprecated/changed rejection, and rollback after a late invalid source selection. Verified with `DATABASE_URL=postgres://crystal_forge:password@127.0.0.1:3042/crystal_forge nix develop ../.. --command cargo test -p cf-server --lib -- --ignored map_existing_stig` from `packages/default`: 3 passed. `nix develop ../.. --command cargo fmt --all --check` and `git diff --check` passed. No commit or push. The worktree also contains an unrelated concurrent modification to `packages/default/crates/cf-server/src/handlers/api/framework_requirements.rs`, left untouched.

2026-08-12 Phase 22 shared-policy validation follow-up: verified the 8 Phase 22 ignored DB tests, plus the complete selected ignored compliance-interchange/framework-requirements suite (34 passed, 0 failed). Also verified cargo fmt --all --check, git diff --check, and SQLX_OFFLINE=true cargo check -p cf-server. Removed unused imports exposed by the final validation pass. The only remaining worktree change is packages/default/crates/cf-server/src/queries/compliance_interchange.rs; no commit or push performed.

2026-08-13: Added and pushed manifest-backed Playwright coverage for real New custom policy → Mappings UI with two queued mappings (commit 00dbc2a0). The successful web-ui check's VM artifacts were not exported into this worktree; result points only to the packaged web-ui output. Next slice is create-mode mapping persistence.

2026-08-13: Corrected 20aa Playwright selectors to wait for asynchronously loaded framework/version options, added policy-card data-policy-id, and changed audit verification to resolve current_version_id from the policy list API before querying persisted mappings. node --check, git diff --check, cargo fmt --manifest-path packages/default/Cargo.toml --all --check, cargo check -p cf-server, and cargo check --manifest-path packages/web-ui/Cargo.toml passed; full web-ui Nix check was interrupted by the 120-second tool timeout before completion.

2026-08-13: Fixed two root causes exposed by 20aa: (1) policy_editor_modal was discarding the create POST response and then re-fetching first-100 list, making new policies invisible above 100; fix inserts entry from refreshed list at front of library. (2) The create endpoint returns a bare deployment_policies row without current_version_id; the fix prefers the entry from the list-response refresh which carries the join-computed current_version_id, enabling the edit modal to load persisted mappings. TASK-421 created to track proper server-side pagination. Latest commits: 52eff5a5, 4d576936. Both cargo check and git diff --check pass. Awaiting next full web-ui Nix run to confirm 20aa green.

2026-08-14 harness repair: canonical fixture JSON parses, but seeder FixtureCves.insights expected Vec while fixture provides an object at line 7426; changed it to opaque serde_json::Value and repaired the ignored parser regression's repository path discovery. Pinned local Nix dev-shell/run-ui-dev/run-ui-frontend Dioxus CLI to nixpkgs commit 09061f... providing dx 0.7.3 with fail-fast version output. Added focused integration-step selection, local manifest/API-layout support, configurable credentials, and authentication waits. The local focused run reaches 20aa but still receives 403 from the framework API despite whoami passing; this is not yet a valid 20aa layer classification. No full web-ui Nix build was rerun.

2026-08-14 Policy Details/Drawer slice: loaded normalized policy requirement mappings from the exact selected policy version with request-generation protection, grouped by framework/release, and rendered relationship, coverage, provenance, rationale, loading/error, and zero-mapping states. Legacy classification is now labeled source/imported metadata. Extended 20aa browser coverage to open the drawer after editor reload and assert persisted normalized mappings. Fixed the fixture hierarchy transaction executor dereference exposed by the Nix server build. Verification: web-ui cargo check passed; cf-server cargo check passed; node --check and git diff --check passed. The authoritative nix web-ui check rebuilt successfully through server compilation but exceeded the 20-minute tool timeout during later VM artifact/design-parity processing; no final check result was observed.

2026-08-14: Browser proof preserved in pushed test-only commit 3ff724b6; TASK-418 worktree clean before P0 feature work. 20a and 20aa both passed with screenshots. Beginning semantic-integrity and bundle requirement-baseline closure slice per user direction.

2026-08-14 checkpoint 1 committed/pushed as f0a855b8: policy semantic digests now incorporate a deterministic sorted mapping digest containing requirement_version_id, relationship, coverage, rationale, provenance, and trust_state. Standalone mapping create/update/delete and transactional import insertion recompute the mutable version digest in-transaction; accepted/deprecated versions remain rejected. Added pure tests for semantic-field changes and insertion-order stability. Verified cargo fmt --all --check, SQLX_OFFLINE=true cargo check -p cf-server, and targeted digest tests (2 passed).

2026-08-14 checkpoint 2 committed/pushed as edafc663: bundle semantic digest now incorporates deterministic requirement-baseline membership; pending digest backfill and baseline insertion refresh bundle digests; ensure_bundle_draft copies exact requirement memberships. Verified cargo fmt and SQLX_OFFLINE=true cargo check -p cf-server. Manual bundle requirement API/UI, immutable import conflicts, and full DB acceptance coverage remain outstanding.

2026-08-14 compatibility regression proof before further P0 work: started isolated DB on 3042 using nix run .#devScripts.db-only. Phase 22 suite ran 8 tests: 7 passed, 1 failed at phase_22_shared_creation_materializes_one_policy_for_three_requirements with persisted digest 6c84270c026f120519bdf402dd45972487aea733cbd55af87f8698f361030729 vs test's plain PolicyVersionCanonical digest a5cf35c21a80fb3e52eebc507bd47aa3c9586336c0fe343ce5d4e01c9c75408a. Ignored cf_native suite ran 9/9 passed; non-ignored cf_native filter had 2 active tests pass and 9 ignored. xccdf filter ran 267 passed, 2 ignored. This confirms the stale Phase 22 assertion and validates the need to restore CF-native semantic_digest compatibility before manual bundle API/UI. No code changes made after edafc663; worktree remains clean.

2026-08-14 component-digest compatibility correction committed/pushed as 192a150d. Added migration 0214 mapping_digest/requirement_digest, restored plain cf-model-json-1 semantic digests, added guarded mutation refreshes and immutable-safe startup component backfills, and corrected Phase 22 semantic/component assertions. Verification: Nix SQLX_OFFLINE=true cargo check -p cf-server passed; cargo fmt --all and git diff --check passed; Phase 22 8/8, CF-native 11/11, focused digest 22/22, non-ignored XCCDF 267 passed with 2 ignored. Full xccdf --include-ignored had one expected artifact-dependent failure because CF_TEST_ANDURIL_STIG_ZIP was unset.

2026-08-14 component-digest compatibility checkpoint committed/pushed as 192a150d. Added mapping_digest and requirement_digest columns, restored semantic_digest to plain cf-model-json-1 contracts, separated mutation refresh/backfill handling, updated mapping/import paths and Phase 22 assertions. Verification reported: cargo fmt, git diff --check, SQLX_OFFLINE cargo check, Phase 22 8/8, CF-native 11/11, digest tests 22/22, XCCDF non-ignored 267 passed/2 ignored. Full ignored XCCDF had one artifact-dependent failure because CF_TEST_ANDURIL_STIG_ZIP was unset. Worktree clean. Manual bundle API/UI remains deferred.

Starting the derived policy draft mapping inheritance slice from clean 192a150d in the dedicated TASK-418 worktree. Production scope is ensure_policy_draft only; all callsites continue using the shared helper.

2026-08-14 derived mapping inheritance verified and pushed as 06b1392e. `cargo fmt --all --check`, `git diff --check`, `SQLX_OFFLINE=true cargo check -p cf-server`, and ignored DB test `queries::deployment_policies::tests::derived_policy_draft_inherits_mappings_and_digests` on PostgreSQL 127.0.0.1:3042 passed. No UI/API/bundle files changed.

2026-08-14 manual bundle requirement baseline server slice implemented in the dedicated worktree from 06b1392e. Added serde-default requirement_version_ids to create/update requests; requirement-only baselines are accepted while completely empty requests retain PolicyRequired validation; exact duplicate/missing requirement IDs are rejected transactionally; requirement membership is written in request order and refreshed via requirement_digest without changing semantic_digest; derived bundle drafts copy and refresh requirement membership; added version-scoped requirement membership query/API; preserved policy membership tables. Added unit validation and ignored DB lifecycle coverage. Verified isolated DB 3042: exact_technical_match_end_to_end 3/3, phase_22 8/8, reviewed_related_stig 2/2, requirement_baseline_lifecycle 1/1. cargo fmt check, git diff check, SQLX_OFFLINE cargo check, and focused validation unit test passed. nix build .#server --no-link was attempted twice and exceeded tool timeouts; no commit or push made. Worktree remains modified and HEAD remains 06b1392e.

2026-08-14 continuation: server baseline API and lifecycle are committed at 209fb0f3/309664b2. Starting the minimal Dioxus bundle baseline selector on the dedicated TASK-418 worktree. Keep policy picker unchanged; use normalized framework/version/search APIs and send exact requirement_version_ids for create/update.

2026-08-15 bundle baseline UI slice implemented in the dedicated worktree: create/edit request models now send requirement_version_ids; new framework-release/search picker selects exact normalized requirement versions independently from policies; edit loads existing draft requirement membership; zero-policy requirement-only bundles are allowed while empty requests remain blocked. Verified web-ui cargo check, server cargo check, cargo fmt --manifest-path packages/default/Cargo.toml --all -- --check, git diff --check, and nix build .#web-ui (171 tests passed, 1 ignored). Changes remain uncommitted by instruction. Focused browser proof for create/edit baseline persistence is still pending.

2026-08-15 browser coverage slice added as 20ab-compliance-bundle-requirement-baseline-roundtrip. It exercises requirement-only creation, reload/policy independence, mixed edit, complete update payloads, release switching with clearing, release-scoped search, requirement edit preserving policies, and empty-baseline blocking. Added v2 normalized fixture release and exposed normalized framework names in bundle framework selection. Static checks and web-ui build pass. Browser verification is incomplete: the authoritative web-ui VM check exceeded the 20-minute command timeout before a final result; local focused run was blocked because run-ui-dev --dev invokes a missing `crystal-forge-server` binary from the `server` flake output. No commit made because browser proof is not green.

2026-08-15 checkpoint committed and pushed as 14f8d962 (feat(web-ui): add bundle requirement baseline editing). Includes 20ab browser coverage, v2 release fixture, Nix-compatible Playwright executable override, and load-time override. Static checks, web-ui cargo check, and nix build .#web-ui pass. Focused local browser execution reached the new test but remains blocked at the cross-origin bundle POST in the local Dioxus/API setup; the authoritative VM check was previously timeout-limited. Branch is pushed for review.

2026-08-15 follow-up committed/pushed as e8c0765c (test(web-ui): harden bundle baseline browser coverage). Added Nix Chromium executable support, configurable load timeout, cross-origin local forwarding, and request-context cookie handling to the focused browser harness. Static checks pass. Local focused browser now reaches the real create API but receives HTTP 403 because the standalone Playwright request context does not inherit the authenticated session; authoritative VM execution remains the required browser proof.

2026-08-15: User confirmed the bundle baseline checkpoint at db520f61 is complete and requested continuation. Existing coverage endpoint/card already exists but currently returns only mapped policy IDs and renders flat rows; the next slice will add read-only mapping evidence and design-aligned hierarchy/details without changing mapping CRUD.

2026-08-15 coverage presentation slice committed/pushed as 9034d55. Bundle coverage now returns trusted selected-policy mapping evidence (policy name/version ID, relationship, coverage, provenance, rationale) per requirement; Compliance card displays mapping evidence and design-aligned Fully covered/Partially covered/Unmapped labels with test IDs. Focused 20ab was extended with authoritative coverage response and expandable-card assertions. Static checks passed: node --check, git diff --check, cargo fmt --all --check, server/web cargo check, and nix build .#server. Focused browser rerun was not conclusive because the local UI navigation timed out at /login before the test began; retry against the newly built server before declaring this checkpoint complete.

2026-08-15 coverage checkpoint browser proof completed. Built updated server with `nix build .#server -o /tmp/task418-server-new`; focused `20ab-compliance-bundle-requirement-baseline-roundtrip` passed 1/1 with dark/light screenshots after asserting authoritative coverage response shape, expandable coverage card, and release-scoped search. Test selector correction was pushed separately as 3c62dbad. Coverage implementation remains at 9034d55; branch is clean after removing generated Tailwind output.

2026-08-15: User confirmed the read-side coverage checkpoint and requested mapping mutation as the next slice. Audit found persisted mapping add/delete already wired and server CRUD validation/immutability present, but the policy UI lacked persisted mapping edit controls.

2026-08-15 mapping mutation checkpoint committed/pushed as 1c31d9d3. Policy Mappings UI now supports persisted mapping edit (relationship, coverage, rationale) and removal, with exact requirement selection retained and immutable rows remaining read-only. Added browser coverage to change Supports/Partial to Implements/Full and remove a second mapping; focused 20aa passed 1/1 with dark/light screenshots. Server validation already enforced allowed relationships/coverage, duplicate uniqueness, trusted manual provenance, and accepted/deprecated immutability. Web UI cargo check, server SQLX_OFFLINE cargo check, cargo fmt check, node --check, and git diff --check passed. Worktree clean.

2026-08-15: User requested the next checkpoint turn the existing STIG importer reconciliation data into a requirement-oriented deterministic reuse workflow. Audit found a reconciliation stage already exists, but its summary is generic, candidate evidence is sparse, and it does not distinguish authoritative/inherited/exact proof classes in the primary summary.

2026-08-15 STIG reconciliation presentation checkpoint committed/pushed as e31ceeba. The existing reconciliation stage now presents requirements rather than controls, distinguishes authoritative/inherited/exact candidate counts, calls out inferred enforcement, shows candidate policy names/confidence/reasons, and keeps proof separate from Refine mapping semantics. Deterministic candidates remain auto-resolved; attention cases and Refine all still use RefinePolicyStep. Verification: web UI cargo check, SQLX_OFFLINE server cargo check, `nix build .#web-ui -L` (171 passed, 1 ignored), and git diff check passed. No dedicated browser import fixture exists in the current manifest, so focused STIG-import browser proof was not runnable in this checkpoint; existing mapping/bundle browser suites remain green from prior checkpoints.

2026-08-15: Manifest registration for 20ac is committed as ba2f39bf. Full NixOS web-ui check reached the Playwright launch but exceeded the execution window before a final result; focused runtime proof remains required.

2026-08-15 focused runtime attempts: direct Playwright execution is available via CF_UI_TEST_STEPS=20ac-stig-import-reconciliation-fixture. Initial attempt exposed a fixture selector defect: the STIG action is inside the Import / Export menu; fixed and pushed as cb756c45. Local run-ui-dev execution then remained blocked by local-auth/bootstrap setup and did not produce screenshots. The authoritative NixOS check was not rerun to completion; focused Playwright PASS and dark/light screenshots remain outstanding.

2026-08-15 focused-check failure reporting fix committed/pushed as 4f7314bf. Top-level integration failures now write fatal.json; Nix wrapper records integration.exit and waits for results/fatal/exit; focused timeout is 180s while normal web-ui remains 1800s. node --check and git diff --check pass. A post-fix focused build was started but the outer command timed out during server compilation before a Playwright result was observed; 20ac matrix expansion remains blocked.

2026-08-15 review remediation pushed as 9e90de5e. Narrowed migration 0215 backfill exceptions to only pending requirement_digest or mapping_digest. Added migration 0216 DB guards for immutable framework and requirement versions, with pending digest finalization and one-time null-to-parent hierarchy construction exception. Framework release digest now includes deterministic requirement semantic digests and DISA import/fixture paths precompute them before release insertion, so same-release requirement content changes conflict without mutating existing releases. Mapping CRUD now scopes nested policy-version routes, forces manual provenance for external creation, permits operator/admin roles, and bundle mapped-policy projection filters trusted mappings. Isolated PostgreSQL 3042 applied migrations 215/216; framework-requirements ignored suite passed 9/9. cargo fmt check, SQLX_OFFLINE cargo check, and git diff check passed. MR !315 restored to Draft.

2026-08-15 follow-up review found framework preview/commit digest mismatch, commit digest derived from policy_records instead of authoritative parsed requirements, permanent finalized requirement reparent escape hatch, and legacy framework digest upgrade compatibility gap. These are now the active remediation scope.

2026-08-15 follow-up remediation pushed as aca32444. Added shared authoritative parsed DISA requirement collector; preview and commit now hash the complete parsed requirement set, independent of policy_records or user selections. Framework digest canonicalization is versioned as cf-model-json-2. Added migration 0217 to reopen/recanonicalize pre-9e90 framework rows and startup backfill, verified on isolated PostgreSQL 3042 with pending count reduced to zero. Added migration 0218 removing finalized requirement reparenting; fixture hierarchy now links while digests are pending and finalizes afterward. Added regression coverage for same-release same-requirement reuse with different artifact SHA, changed requirement conflict, finalized reparent rejection, and digest backfill. Verification: migrations 217/218 applied; framework digest backfill test passed; framework preview and requirement immutability DB tests passed; cargo fmt, SQLX_OFFLINE cargo check, and git diff check passed. MR remains Draft at aca32444.

2026-08-15 review found concurrent startup backfill race, vacuous legacy-backfill test, DISA external_id incorrectly using canonical V-ID, and production STIG hierarchy not persisted. These are now the active remediation scope.

2026-08-15 v4 checkpoint committed/pushed as 8b7e445e. Framework canonicalization now includes complete group/rule content plus hierarchy edges; migration 0220 reopens cf-model-json-3 rows and permits pending requirement content reconstruction while the parent framework remains pending. Startup backfill reconstructs legacy DISA topology from persisted source artifacts, preserves XCCDF Rule external IDs, and uses the same complete requirement set for framework identity. Verification: cargo fmt --all --check, SQLX_OFFLINE=true cargo check -p cf-server, and git diff --check passed. Isolated DB legacy topology test passed after correcting the fixture to use XCCDF <version> element. The broader framework_requirements DB filter had 8/11 passing; 3 failures were RowNotFound because the isolated database lacked seeded users after an attempted reset. Untracked packages/web-ui/assets/tailwind.css was intentionally not staged.

2026-08-15 hardening checkpoint committed/pushed as 537668d9. v4 backfill now classifies legacy DISA state by persisted publisher, rejects missing/unparseable/non-DISA artifacts, verifies parsed source/release identity before mutation, reconstructs absent rule versions, and verifies persisted node/edge topology before finalizing. Added 0221 to narrow 0220's pending-framework exception so structural identity fields remain immutable. Updated the topology DB test to use identify_framework()-derived key/release; it passed. cargo fmt, SQLX_OFFLINE cargo check, git diff check, nix build .#server, and nix build .#web-ui passed. nix flake check --keep-going exceeded the 10-minute execution limit while building VM checks, so no final flake-check result. Negative DB tests and properly seeded 11/11 framework suite remain outstanding; the current isolated database is unseeded after the prior reset attempt. tailwind.css remains intentionally untracked.

2026-08-15 metadata-authority checkpoint committed/pushed as 15d1c6d3. Parsed DISA identity now repairs pending framework version/title and lineage publisher before v4 finalization; 0222 permits only those pending framework metadata fields alongside digest metadata. Framework lineage upsert now retains an authoritative non-null publisher on canonical-key reuse. The legacy topology DB test now proves stored version/title/publisher equal identify_framework() output and passed. SQLX_OFFLINE cargo check, cargo fmt, and git diff check passed. Full v3->0220/0221->v4->reimport and negative DB coverage still remain; no claim of 11/11 suite or flake-check completion. tailwind.css remains untracked.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
created: 2026-08-14 02:40
---
2026-08-14 focused 20aa follow-up: restarted run-ui-dev with repaired fixture seeding and pinned dx 0.7.3. Added focused-run auth preflight and credentials include for cross-origin API reads (commit 8a1c9b4e, pushed). The test now authenticates and reaches the framework API successfully, but the first data boundary fails because the fixture seeder loads no framework with canonical_source_key web-ui-mapping-roundtrip; MAP-1/MAP-2 are absent from the seeded database. This is a fixture/API data setup blocker, not yet a policy UI or mapping persistence failure. Local services were stopped after verification.
---

created: 2026-08-14 03:02
---
2026-08-14 fixture repair complete: added an optional normalized compliance fixture shape to the canonical JSON with framework web-ui-mapping-roundtrip, release web-ui-mapping-roundtrip-v1, and requirements MAP-1/MAP-2. Extended fixture seeding to upsert framework lineages, versions, requirement lineages, and requirement versions idempotently using the existing digest/query helpers. Focused 20aa now passes 1/1 with dark/light captures after selecting Security controls and resolving current_version_id from the production list response. Focused runs now skip design-parity capture automatically when CF_UI_TEST_STEPS is set. Verification: targeted fixture deserialization test passed, cargo fmt --all --check passed, node --check passed, git diff --check passed. Pushed commit 26fd22da.
---

created: 2026-08-14 03:19
---
2026-08-14 20aa closure / visual parity slice: closed 20aa debugging after focused 1/1 pass. Refined the Policy Mappings tab toward design commit 861fd877: explanatory intro, framework/version grouping, compact requirement rows, relationship/coverage chips, rationale/provenance presentation, read-only treatment, and purple-accent Add mapping editor. Fixture hierarchy seeding is now order-independent via two-pass parent resolution. Focused 20aa was rerun against current UI with the existing server + pinned dx frontend and passed 1/1 with dark/light captures. Verification: cargo check --manifest-path packages/web-ui/Cargo.toml, cargo fmt --all --check, node --check, git diff --check. Commits 229f29e4 and 5acfdc09 pushed. Full nix build .#web-ui intentionally not run. Policy Add/Edit mapping functionality is complete; visual parity remains in progress.
---

created: 2026-08-14 03:37
---
2026-08-14 final Mappings-tab parity slice: Add Mapping is now collapsed by default and expands on click; Cancel closes without mutation; successful pending/persisted adds close the editor and refresh mapping count. Empty state uses the informational zero-mapping callout. Editor now matches the purple shell/gap, selected requirement card with Change/root-parent context, descriptive Implements/Supports/Provides evidence for cards, Full/Partial segmented control, rationale label/placeholder, and explicit Cancel/Add mapping footer. Focused 20aa was updated for the interaction and passed 1/1 with dark/light captures. Verification: cargo check --manifest-path packages/web-ui/Cargo.toml, cargo fmt --all --check, node --check, git diff --check. Pushed commit 2fb9c08e. Full nix web-ui build not run. Policy Add/Edit Mappings UI is complete; next area is Policy Details/Drawer.
---

author: opencode
created: 2026-08-15 15:59
---
Opened Draft MR !315: https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/315. Focused `20ac-stig-import-reconciliation-fixture` NixOS check passes 1/1 with dark and light screenshots. TASK-418 remains open; remaining reconciliation UX and acceptance work is listed in the MR.
---

author: opencode
created: 2026-08-15 16:05
---
Starting the post-deployment semantic-integrity checkpoint on MR !315. Scope is limited to immutable same-release framework handling, immutable requirement-version reuse/conflict behavior, and published bundle requirement-count fallback. Existing migrations will not be edited; schema changes, if required, will use a new migration.
---

author: opencode
created: 2026-08-15 16:14
---
Pushed semantic-integrity checkpoint `dca95480` to MR !315. Same-release preview/commit now compare framework semantic digests and reuse identical releases; conflicting release content returns `ReleaseConflict`/`FRAMEWORK_RELEASE_CONFLICT`. Requirement-version insertion now reuses identical digests, rejects changed content with `REQUIREMENT_VERSION_CONFLICT`, and never updates immutable fields. Published-only bundle summaries now fall back to `current_published_version_id`. No migration files were edited or added.
---

author: opencode
created: 2026-08-15 16:14
---
Verification: framework query suite 9 passed; Phase 22 8/8 passed; Phase 21 exact technical suite 3 passed; Phase 23 reviewed-related suite 2 passed; bundle baseline lifecycle passed; SQLX_OFFLINE cargo check passed; cargo fmt and git diff check passed. XCCDF filter remains blocked by the pre-existing missing `CF_TEST_ANDURIL_STIG_ZIP` environment variable.
---

created: 2026-08-15 16:57
---
Review remediation checkpoint: addressed findings #1-#6 and the framework release digest blocker in commit 9e90de5e. MR remains Draft pending broader review and verification.
---

created: 2026-08-15 17:29
---
Follow-up review remediation pushed as aca32444: framework preview/commit now share the complete parsed requirement identity, policy selection cannot alter framework release identity, finalized requirement versions cannot be reparented, and pre-9e90 framework digests are recanonicalized through an upgrade migration plus startup backfill.
---
<!-- COMMENTS:END -->

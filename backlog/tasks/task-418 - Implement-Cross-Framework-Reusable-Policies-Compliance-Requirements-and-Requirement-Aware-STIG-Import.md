---
id: TASK-418
title: >-
  Implement Cross-Framework Reusable Policies, Compliance Requirements, and
  Requirement-Aware STIG Import
status: In Progress
assignee:
  - agent
created_date: '2026-08-11 17:37'
updated_date: '2026-08-15 02:33'
labels: []
milestone: m-22
dependencies:
  - TASK-412
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
## Implementation Plan

### Current-system facts (researched 2026-08-11)

| Fact | Value |
|------|-------|
| Next migration | **0211** |
| Framework storage today | Free-text string on `compliance_bundles.framework` / `compliance_bundle_versions.framework` — no dedicated table |
| Requirement storage today | SRG/CCI identifiers in `compliance_metadata JSONB` on `deployment_policy_versions` — no dedicated requirements table |
| Mapping storage today | None — no schema table |
| Digest algorithm | `cf-model-json-1` / `sha-256`; canonical key-sorted JSON objects, arrays in input order |
| Digest sentinel | `'pending'` — SQL sets it, Rust computes the real value within the same transaction |
| Advisory lock style | `pg_advisory_xact_lock(hashtextextended($1, 0))` with sorted+deduped string keys |
| Immutability | `accepted`/`deprecated` versions write-protected by triggers; only `incomplete`/`draft`/`interim` mutable |
| Design authority | `docs/design/CrystalForge/data-mappings.js`, `PoliciesView.jsx`, `ImportStigModal.jsx` |

Key existing modules:
- `src/compliance/xccdf/` — parser, importer, reconciliation, export_models, xml_writer, import_models
- `src/compliance/digest.rs` — `PolicyVersionCanonical`, `BundleVersionCanonical`, `write_policy_version_digest`
- `src/compliance/canonical.rs` — `canonicalize_json()`, `semantic_digest()`
- `src/compliance/interchange.rs` — `CANONICALIZATION_VERSION`, `MAX_XCCDF_UPLOAD_BYTES`
- `src/queries/compliance_interchange.rs` — `commit_foreign_import`, `commit_cf_native_import`
- `packages/web-ui/src/views/compliance.rs` — 4,271 lines, all existing import/assignment UI
- `packages/web-ui/src/views/policies.rs` — policy list/drawer
- `packages/web-ui/src/components/compliance/` — RefinePolicyStep, ImportReview

### Delivery sequence

**Phase A — Database foundations (migrations 0211–0213)**

Migration 0211: `compliance_frameworks` and `compliance_framework_versions`
```sql
compliance_frameworks:
  id UUID PRIMARY KEY DEFAULT gen_random_uuid()
  name TEXT NOT NULL
  publisher TEXT
  canonical_source_key TEXT NOT NULL UNIQUE   -- e.g. "disa-anduril-nixos-stig"
  description TEXT
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()

compliance_framework_versions:
  id UUID PRIMARY KEY DEFAULT gen_random_uuid()
  framework_id UUID NOT NULL REFERENCES compliance_frameworks ON DELETE RESTRICT
  version TEXT NOT NULL
  canonical_release_key TEXT NOT NULL         -- e.g. "V1R1"
  title TEXT
  published_at TIMESTAMPTZ
  source_artifact_id UUID REFERENCES compliance_source_artifacts ON DELETE RESTRICT
  semantic_digest TEXT NOT NULL DEFAULT 'pending'
  digest_algorithm TEXT DEFAULT 'sha-256'
  canonicalization_version TEXT DEFAULT 'cf-model-json-1'
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
  UNIQUE (framework_id, canonical_release_key)  -- typed conflict guard
```

Migration 0212: `compliance_requirements` and `compliance_requirement_versions`
```sql
compliance_requirements:
  id UUID PRIMARY KEY DEFAULT gen_random_uuid()
  framework_id UUID NOT NULL REFERENCES compliance_frameworks ON DELETE RESTRICT
  canonical_requirement_key TEXT NOT NULL     -- adapter-determined: V-268137, SC-45, 5.1.8
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
  UNIQUE (framework_id, canonical_requirement_key)

compliance_requirement_versions:
  id UUID PRIMARY KEY DEFAULT gen_random_uuid()
  requirement_id UUID NOT NULL REFERENCES compliance_requirements ON DELETE RESTRICT
  framework_version_id UUID NOT NULL REFERENCES compliance_framework_versions ON DELETE RESTRICT
  external_id TEXT NOT NULL               -- same as canonical key or release-specific ID
  title TEXT
  description TEXT
  kind TEXT NOT NULL                      -- family/control/enhancement/group/rule/section/domain/practice
  parent_requirement_version_id UUID REFERENCES compliance_requirement_versions ON DELETE RESTRICT
  severity TEXT
  check_text TEXT
  fix_text TEXT
  metadata JSONB NOT NULL DEFAULT '{}'   -- CCI/SRG/references/platforms/etc.
  semantic_digest TEXT NOT NULL DEFAULT 'pending'
  digest_algorithm TEXT DEFAULT 'sha-256'
  canonicalization_version TEXT DEFAULT 'cf-model-json-1'
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
  UNIQUE (requirement_id, framework_version_id)  -- one version per release per lineage
```

Migration 0213: `policy_requirement_mappings` and `compliance_bundle_version_requirements`
```sql
policy_requirement_mappings:
  id UUID PRIMARY KEY DEFAULT gen_random_uuid()
  policy_version_id UUID NOT NULL REFERENCES deployment_policy_versions ON DELETE RESTRICT
  requirement_version_id UUID NOT NULL REFERENCES compliance_requirement_versions ON DELETE RESTRICT
  relationship TEXT NOT NULL              -- implements/supports/provides_evidence_for
  coverage TEXT NOT NULL                  -- full/partial
  rationale TEXT
  provenance TEXT NOT NULL DEFAULT 'manual'  -- manual/imported/inherited/inferred/suggested
  source_artifact_id UUID REFERENCES compliance_source_artifacts ON DELETE SET NULL
  trust_state TEXT NOT NULL DEFAULT 'trusted'  -- trusted/suggested (suggested = not yet accepted)
  created_by UUID REFERENCES users ON DELETE SET NULL
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
  UNIQUE (policy_version_id, requirement_version_id)  -- one mapping per pair
  -- Immutability trigger: block UPDATE/DELETE when policy version is accepted/deprecated

compliance_bundle_version_requirements:
  bundle_version_id UUID NOT NULL REFERENCES compliance_bundle_versions ON DELETE CASCADE
  requirement_version_id UUID NOT NULL REFERENCES compliance_requirement_versions ON DELETE RESTRICT
  selected BOOLEAN NOT NULL DEFAULT true
  requirement_order INTEGER NOT NULL
  PRIMARY KEY (bundle_version_id, requirement_version_id)
```

GIN index on `compliance_requirement_versions.metadata` for CCI/SRG search.
Full-text search index on `title || ' ' || external_id` for requirement search.

**Phase B — Rust domain models and digest**

- `src/compliance/requirements.rs` — `RequirementVersionCanonical`, `compute_requirement_version_digest()`
- `src/compliance/framework.rs` — `FrameworkVersionCanonical`, `compute_framework_version_digest()`
- `src/compliance/mapping.rs` — mapping create/read/delete query helpers; mutability guard (reject if policy version is accepted/deprecated)
- Extend `digest.rs` startup backfill to cover new `'pending'` rows

**Phase C — DISA STIG adapter**

New module: `src/compliance/xccdf/disa_stig_adapter.rs`
- `DisaStigAdapter::identify_framework(parsed: &ParsedXccdf) -> Option<FrameworkIdentity>`
  - Detects DISA STIGs by benchmark ID prefix, description keywords, and presence of V-IDs / SRG/CCI references
  - Returns `canonical_source_key`, `canonical_release_key`, `publisher = "DISA"`
- `DisaStigAdapter::canonical_key_for_rule(rule: &ParsedRule) -> String`
  - Prefers V-ID from `<ident system="...stig_id...">` or identifier list; falls back to group ID
- `DisaStigAdapter::hierarchy_for_rule(rule: &ParsedRule) -> Vec<HierarchyNode>`
  - Maps group → rule with CCI/SRG metadata

**Phase D — Requirement-aware import reconciliation**

Extend `src/compliance/xccdf/importer.rs` and `queries/compliance_interchange.rs`:

**Preview path** (mutation-free, read-only):
1. `reconcile_framework_release(pool, adapter, parsed) -> FrameworkReconciliation`
   - Check exact SHA-256 → `EXACT_ARTIFACT` (full re-use)
   - Check `canonical_source_key` + `canonical_release_key` → existing release or new
   - Compute proposed semantic digest; compare to existing → `RELEASE_CONFLICT` if same key but different content
2. `reconcile_requirements(pool, framework_version_id, rules[]) -> Vec<RequirementReconciliation>`
   - Batch query all `canonical_requirement_key`s for this framework
   - Classify each as: `EXISTING_UNCHANGED / EXISTING_CHANGED / NEW_REQUIREMENT / IDENTITY_CONFLICT`
3. `reconcile_policy_candidates(pool, requirement_id, rule) -> PolicyReconciliation`
   - Check accepted `policy_requirement_mappings` for this requirement (highest confidence)
   - Check inherited mapping from previous unchanged requirement version
   - Check exact technical implementation match (normalized config hash)
   - Return ordered `PolicyCandidate[]` with `match_type` and `confidence`

**Commit path** (TOCTOU-safe):
1. Re-validate artifact digest
2. Re-run adapter identification
3. Re-run requirement reconciliation against current DB state
4. Validate user `RequirementDecision[]` against reparsed identities
5. Acquire advisory locks (sorted+deduped): framework lineage, framework version, requirement lineages, requirement versions, policy version IDs, bundle lineage, bundle version
6. Upsert framework lineage and version (ON CONFLICT DO NOTHING)
7. Upsert requirement lineages and versions
8. Insert `compliance_bundle_version_requirements` rows
9. Create/reuse policy lineages and versions (per decision)
10. Insert `policy_requirement_mappings` rows
11. Source artifact + source_object_mappings
12. Audit events
13. Atomic commit

**Phase E — Backend APIs**

New route group in `compliance.rs` handler (or new handler file `framework_requirements.rs`):

```
GET  /api/v1/compliance/frameworks
GET  /api/v1/compliance/frameworks/:id
GET  /api/v1/compliance/frameworks/:id/versions
GET  /api/v1/compliance/framework-versions/:fv_id/requirements?q=&kind=&limit=&cursor=
GET  /api/v1/compliance/requirement-versions/:rv_id
GET  /api/v1/compliance/requirement-versions/:rv_id/children

GET  /api/v1/compliance/bundle-versions/:bv_id/requirement-coverage

GET  /api/v1/policy-versions/:pv_id/requirement-mappings
POST /api/v1/policy-versions/:pv_id/requirement-mappings        (draft only)
PUT  /api/v1/policy-versions/:pv_id/requirement-mappings/:m_id  (draft only)
DELETE /api/v1/policy-versions/:pv_id/requirement-mappings/:m_id (draft only)
```

Coverage endpoint response shape:
```json
{
  "bundle_version_id": "...",
  "total_requirements": 40,
  "full": 28,
  "partial": 6,
  "unmapped": 6,
  "by_hierarchy": [ { "kind":"group", "external_id":"...", "title":"...", "coverage":"full", "children":[] } ]
}
```

Requirement search: `?q=<text>&kind=rule&limit=25&cursor=<opaque>` — server-side full-text + trigram on `title || ' ' || external_id || ' ' || metadata->>'cci_ids'`

**Phase F — Web UI**

1. **Policy modal Mappings tab** — new `MappingsTab` component:
   - Framework selector → calls `GET /frameworks` (cached)
   - Requirement search input → debounced `GET /framework-versions/:fv_id/requirements?q=`
   - Breadcrumb from `GET /requirement-versions/:rv_id/children` ancestors
   - Relationship and coverage selects
   - Rationale textarea
   - POST/DELETE to mapping endpoints (blocked when policy version is accepted)
   - Display: mappings grouped by framework, each showing external_id, title, relationship chip, coverage badge, provenance label

2. **Policy list/drawer** — "Mapped to N requirements" count from `GET /policy-versions/:pv_id/requirement-mappings` (count only for list view)

3. **Policy grouping** — derive from normalized mappings → `requirement → ancestor family` rather than legacy `control_family` field

4. **Compliance view: Requirement coverage card** — call `GET /bundle-versions/:bv_id/requirement-coverage`; render full/partial/unmapped chip counts; expandable hierarchy rows

5. **Bundle add/edit policy selector** — split into "Mapped to \<framework\>" and "Custom addition" sections using `GET /framework-versions/:fv_id/requirements` to check mapping

6. **STIG import UI** — new "reconciliation" step before per-control refinement:
   - Summary cards: "N reused", "N ready", "N need attention"
   - Attention list: requirements needing human decision (no auto-resolved candidate)
   - "Refine all" escape hatch
   - Preserve existing refine workflow for manually inspected controls

**Phase G — Tests**

DB-gated tests in `queries/compliance_interchange.rs` or new `queries/framework_requirements.rs`:
- Framework uniqueness (duplicate canonical_source_key → conflict)
- Framework version uniqueness (duplicate canonical_release_key → conflict)
- Requirement lineage reuse across framework releases
- Mapping create/update/delete on draft policy version
- Mapping blocked on accepted policy version
- Bundle coverage: full / partial / unmapped
- Exact re-import idempotency (0 duplicates)
- New release: lineage reused, requirement versions created, unchanged mappings inherited
- Concurrent identity race (two goroutines importing same framework simultaneously)
- Complete rollback on import failure

Unit tests (offline):
- `DisaStigAdapter::canonical_key_for_rule` with various STIG rule fixtures
- `RequirementVersionCanonical::compute_digest` stability and field coverage
- Coverage logic: `full` requires `implements + full`; `partial` from any other mapping

**Phase H — Verification**

```bash
cargo fmt --all --check
nix build .#server
nix build .#web-ui
nix flake check --keep-going
```

Targeted DB tests:
```bash
DATABASE_URL=... cargo test -p cf-server --lib -- --ignored queries::framework_requirements
DATABASE_URL=... cargo test -p cf-server --lib -- --ignored queries::compliance_interchange
```

### Sequencing and checkpoints

1. Migrations 0211–0213 → `nix develop --command cargo sqlx migrate run` → sqlx offline metadata update
2. Rust domain models + digest → `SQLX_OFFLINE=true cargo check -p cf-server`
3. DISA adapter + preview reconciliation → offline unit tests green
4. Commit path → DB-gated tests green (idempotency, concurrency, rollback)
5. APIs → `SQLX_OFFLINE=true cargo test -p cf-server --lib`
6. Web UI → `nix build .#web-ui` green
7. Full flake check

### Risk notes

- Mapping immutability trigger must interlock with the existing `trigger_guard_policy_version_state_immutability` pattern from migrations 0197/0202 — do not duplicate trigger logic, extend it
- `semantic_digest = 'pending'` sentinel is a load-bearing invariant; any new versioned entity must use it and the startup backfill must cover it
- The DISA adapter heuristic is framework-detection, not format-detection — keep it separate from the generic XCCDF parser
- Requirement search must be bounded (`LIMIT 50` max) and indexed — no full table scan on a large STIG catalog
- UI mapping editor must handle the offline state gracefully (failed search, failed create) without optimistic mutation

2026-08-11 continuation: wire the existing normalized framework/requirement query layer into foreign DISA STIG preview and atomic commit before further UI work. The preview will provide real reconciliation states/candidates; the commit will reparse as it already does, use the normalized upsert helpers in the same transaction, and make exact-artifact imports idempotent. Then connect the reconciliation modal and bundle selector to these APIs, followed by targeted server/web builds.

2026-08-11 continuation plan: complete the mutation-free release-diff projection first. Pass adapter-derived requirement canonicals/digests into reconciliation, compare against the prior framework release even when previewing a new release, return changed and removed states, and offer inherited candidates only for unchanged requirements. Add focused query tests. Defer commit-time reuse of immutable accepted policy versions until its required derived-draft behavior is confirmed against existing workflow, because silently mutating accepted versions would violate the task's immutability criterion.

2026-08-11 reuse increment: at foreign-import commit, resolve each `MapExisting` source policy version to a mutable derived draft using the established `ensure_policy_draft(..., EnsureMutable)` workflow when the source version is immutable. Use the effective draft version consistently for bundle membership, normalized mapping, source-object mappings, and reuse counts. Validate that the selected source is an eligible trusted mapping for the relevant previous requirement version before deriving or persisting. Add DB-gated coverage for immutable selected-policy reuse.

2026-08-11 confirmed follow-up scope: preserve the full trusted inherited mapping contract (relationship, coverage, rationale) on the derived draft; make commit eligibility exactly match preview (accepted and current published only); deduplicate effective policy versions for bundle membership while retaining per-requirement mappings; correct actual creation/reuse result counts; add the listed DB integration cases and execute both DB suites before proceeding to exact technical-match candidates.

2026-08-11 validation correction: MapExisting is now explicitly restricted to a trusted mapping on the unchanged prior requirement where the submitted policy version is both accepted and the lineage's current published version. Every accepted source is then resolved through `ensure_policy_draft(..., EnsureMutable)`; client-submitted mutable versions are rejected. Add a complete DB integration fixture covering shared-policy reuse, preservation of mapping semantics, derived draft membership/mapping, deterministic membership order, accounting, and rejected superseded/deprecated/stale versions before starting technical matching.

2026-08-11 DB-proof slice: do not start technical, fuzzy, or crosswalk matching. Add focused DB-gated lifecycle tests for inherited MapExisting success, exact mapping-semantics inheritance, existing draft reuse, source rejection cases, multi-requirement dedup/order/accounting, and full transaction rollback. Run the complete existing compliance interchange and framework requirements ignored suites, formatting/diff checks, and prescribed server build/check; commit and push this test slice separately only after green.

### 2026-08-11 MapExisting DB-proof slice
- Extend `cf-server`'s existing ignored `queries::compliance_interchange` database fixtures and commit-path tests only; do not alter production matching behavior.
- Cover accepted/current trusted inherited reuse, inherited semantics and existing-draft preservation, rejection of invalid source states/content, shared effective-draft bundle deduplication/order/accounting, and transaction rollback.
- Verify with the requested isolated `DATABASE_URL` against the targeted ignored DB suite, plus formatting and diff checks.

Next slice: persist create-mode queued requirement mappings atomically with new policy creation. Extend the create-policy request/handler/query path to accept validated mapping payloads, create the policy first, insert mappings in the same transaction, and preserve all-or-nothing behavior. Add focused server tests and update the browser round-trip step to verify persisted mappings after reload. Keep accepted-version immutability and existing edit-mode CRUD unchanged.

2026-08-14 P0 closure slice approved by user: preserve verified browser proof first (test-only commit 3ff724b6 pushed); then implement policy-version mapping-inclusive canonical digests and same-transaction draft mutation recomputation with immutable-version guards; extend bundle canonical digests and ensure_bundle_draft to preserve exact requirement memberships; add manual bundle create/update API support for independent requirement_version_ids including zero-policy baselines; add minimal baseline UI using existing framework/version/requirement search; harden framework-release and requirement-version reimport identity conflicts and remove mutable semantic upserts; add targeted DB acceptance coverage for all invariants. Sequence checkpoints: mapping digest, bundle digest/draft, API/server baseline, UI baseline, immutable import conflicts, regression coverage. Verification gates: isolated PostgreSQL on 3042 targeted tests, cargo fmt --all --check, git diff --check, SQLX_OFFLINE=true cargo check -p cf-server, cargo check -p web-ui, nix build .#server, nix build .#web-ui. Do not start fuzzy matching or unrelated UI.

2026-08-14 derived policy draft mapping inheritance: in ensure_policy_draft, copy all policy_requirement_mappings immediately after inserting the new draft, then compute the draft digest with copied mappings present. Add one ignored DB lifecycle test covering three mappings, semantic/mapping digest parity, distinct mapping IDs, draft-only mutation, and accepted-source mutation rejection. Verify fmt, diff check, SQLX_OFFLINE cargo check, and targeted DB test on port 3042.

Next checkpoint — bundle requirement-baseline UI: extend create/edit bundle request models with requirement_version_ids; add a framework/version-scoped requirement selector using existing framework/version/search APIs; allow zero-policy requirement-only bundles while preserving existing policy selection; submit exact requirement IDs and display selected baseline count. Verify web-ui cargo check, server cargo check, focused browser coverage if available, then authoritative web-ui build.
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
<!-- COMMENTS:END -->

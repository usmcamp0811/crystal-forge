---
id: TASK-418
title: >-
  Implement Cross-Framework Reusable Policies, Compliance Requirements, and
  Requirement-Aware STIG Import
status: In Progress
assignee:
  - agent
created_date: '2026-08-11 17:37'
updated_date: '2026-08-11 19:20'
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
<!-- SECTION:NOTES:END -->

# TASK-417 - Implement Cross-Framework Reusable Policies, Compliance Requirements, and Requirement-Aware STIG Import

## Summary

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

---

# Background

MR !313 established the versioning and lifecycle foundation required for this work:

- policy lineages
- immutable policy versions
- bundle lineages
- immutable bundle versions
- derived drafts
- publication states
- trust state
- semantic digests
- source artifacts
- source object mappings
- XCCDF / CF-native import and export
- atomic import reconciliation
- exact policy-version bundle membership
- assignment resolution
- advisory locking
- deletion protections

This task builds the normalized compliance graph on top of that foundation.

Do not weaken or bypass any !313 immutability, trust, versioning, reconciliation, or digest guarantees.

---

# Core Domain Model

The long-term mental model is:

```text
Framework
    |
    +-- Framework Version
            |
            +-- Requirement Version
                    ^
                    |
             Policy Mapping
                    |
              Policy Version
                    |
                    +-- Bundle Version

Bundle Version
    |
    +-- Requirement Version
```

In plain language:

```text
Frameworks define requirements.

Policies define reusable technical behavior.

Mappings state which requirements a policy implements,
supports, or provides evidence for.

Bundles select exact policy versions and exact requirements
to form a compliance baseline.

Imports reconcile against existing requirements and policies
before creating anything new.
```

---

# Architectural Invariants

The implementation must maintain all of the following.

## Policy ownership

A policy does not belong to exactly one compliance framework.

A policy may:

- have no compliance mappings
- map to one requirement
- map to many requirements in one framework
- map to requirements in multiple families of one framework
- map to requirements in multiple frameworks
- be selected by multiple compliance bundles

Do not replace:

```text
framework: NIST
```

with:

```text
frameworks: [NIST, DISA, CIS]
```

That preserves the wrong abstraction.

The correct abstraction is:

```text
Policy Version
    |
    +--> Mapping --> Requirement Version --> Framework Version
    +--> Mapping --> Requirement Version --> Framework Version
```

## Requirement ownership

A compliance requirement belongs to an authoritative framework lineage.

Requirement hierarchy belongs to the framework, not the policy.

Example:

```text
NIST 800-53 Rev 5

SA
└── SA-10
    └── SA-10(1)
```

The policy maps to `SA-10` or `SA-10(1)`.

The policy does not store:

```text
family = SA
```

as authoritative compliance meaning.

## Exact versions

Mappings must reference exact immutable policy versions and exact requirement versions.

Do not silently substitute the latest policy version.

## Requirement without implementation

A compliance requirement may exist with no executable policy.

Do not create placeholder technical policies merely to preserve framework completeness.

## Crosswalk safety

A framework requirement crosswalk does not automatically imply a policy mapping.

Given:

```text
Policy P -> Requirement A
Requirement A -> Requirement B
```

do not automatically conclude:

```text
Policy P -> Requirement B
```

A crosswalk may produce a suggestion requiring review.

---

# Phase 1 - Normalized Framework Model

Create a first-class framework lineage.

Suggested conceptual schema:

```text
compliance_frameworks
    id
    name
    publisher
    canonical_source_key
    description
    created_at
```

Examples:

```text
NIST 800-53
DISA RHEL 9 STIG
DISA Anduril NixOS STIG
CIS NixOS Benchmark
CMMC 2.0
```

Do not model interchange formats as frameworks.

For example:

```text
XCCDF
```

is a source format, not a compliance framework.

Add appropriate uniqueness constraints around authoritative framework identity.

---

# Phase 2 - Framework Versions

Create immutable framework versions/releases.

Suggested conceptual schema:

```text
compliance_framework_versions
    id
    framework_id
    version
    canonical_release_key
    title
    published_at
    source_artifact_id
    semantic_digest
    created_at
```

Examples:

```text
NIST 800-53 Rev 5
DISA Anduril NixOS STIG V1R1
DISA Anduril NixOS STIG V1R2
```

Requirements imported from a specific release must refer to that exact framework version.

Prevent duplicate authoritative release identities.

If two different artifacts claim the same authoritative release identity but differ semantically, return a typed conflict instead of silently creating duplicate releases.

---

# Phase 3 - Requirement Lineages

Create stable compliance requirement lineages.

Suggested conceptual schema:

```text
compliance_requirements
    id
    framework_id
    canonical_requirement_key
    created_at
```

The canonical requirement key must be determined by the framework/source adapter.

Do not assume one global identifier scheme.

Examples:

```text
NIST:
    SC-45

DISA STIG:
    V-268137

CIS:
    5.1.8
```

For DISA STIG content, stable vulnerability identifiers such as `V-xxxxx` should normally be preferred when authoritative for the requirement lineage.

Preserve all source identifiers regardless of which identifier is canonical.

---

# Phase 4 - Requirement Versions

Create immutable requirement versions.

Suggested conceptual schema:

```text
compliance_requirement_versions
    id
    requirement_id
    framework_version_id

    external_id
    title
    description
    kind

    parent_requirement_version_id

    severity
    check_text
    fix_text

    metadata
    semantic_digest

    created_at
```

A requirement appearing in multiple framework releases should retain one lineage with separate immutable versions.

Example:

```text
Requirement lineage:
V-268137

Versions:
    Anduril STIG V1R1 representation
    Anduril STIG V1R2 representation
```

This must allow CF to determine whether the requirement changed between releases.

---

# Phase 5 - Generic Requirement Hierarchy

Requirement hierarchy must not be NIST-specific.

Supported examples include:

```text
NIST
Family
  -> Control
      -> Enhancement
```

```text
DISA STIG
Group
  -> Rule
```

with references to:

```text
CCI
SRG
```

and:

```text
CIS
Section
  -> Subsection
      -> Recommendation
```

Use generic hierarchy fields such as:

```text
kind
parent_requirement_version_id
```

Do not hard-code hierarchy depth.

The UI must be able to render arbitrary framework hierarchy using the same reusable components.

---

# Phase 6 - Policy-to-Requirement Mappings

Create a first-class many-to-many relationship between exact policy versions and requirement versions.

Suggested conceptual schema:

```text
policy_requirement_mappings
    id
    policy_version_id
    requirement_version_id

    relationship
    coverage
    rationale

    provenance
    source_artifact_id
    trust_state

    created_by
    created_at
```

## Relationship

Initial supported values:

```text
implements
supports
provides_evidence_for
```

Semantics:

### Implements

The policy directly implements the technical behavior represented by the requirement.

### Supports

The policy contributes to satisfying the requirement but does not independently satisfy the full requirement.

### Provides evidence for

The policy gathers or generates evidence relevant to determining compliance with the requirement.

## Coverage

Initial supported values:

```text
full
partial
```

Example:

```text
Policy:
Require synchronized system time

Mappings:

SC-45
    implements
    full

AU-8
    supports
    partial
```

---

# Phase 7 - Mapping Provenance

Mappings must retain provenance.

Initial provenance should support at least:

```text
manual
imported
inherited
inferred
```

The architecture must also leave room for:

```text
suggested
crosswalk-derived
```

A mapping derived from an imported authoritative artifact should retain its source artifact relationship.

Example:

```text
Source:
Imported from DISA XCCDF
```

Suggested mappings must remain separate from accepted mappings until explicitly accepted.

---

# Phase 8 - Mapping Immutability and Versioning

Mappings associated with an accepted/published policy version must not be mutable in place.

Required behavior:

```text
Accepted Policy Version
    mappings read-only
          |
          v
Create derived draft
          |
          v
Draft Policy Version
    mappings editable
```

Editing mappings on an immutable policy must use the existing derived-draft workflow from !313.

Do not weaken publication guards.

A mapping change must be included in deterministic policy-version semantic state.

It is acceptable to maintain separate component digests such as:

```text
implementation_digest
mapping_digest
```

if useful internally, but the accepted policy version must have deterministic immutable semantics.

---

# Phase 9 - Bundle Requirement Membership

Add explicit requirement membership to bundle versions.

Suggested conceptual schema:

```text
compliance_bundle_version_requirements
    bundle_version_id
    requirement_version_id
    selected
    requirement_order
```

This relationship is separate from existing:

```text
compliance_bundle_version_policies
```

A framework bundle therefore contains two related but distinct sets:

```text
Requirement baseline:
    What the framework requires

Policy set:
    Which technical implementations CF selected
```

Example:

```text
DISA Anduril NixOS STIG V1R2

Requirements:
    V-111
    V-222
    V-333

Policies:
    P17
    P24
```

Do not require every requirement to have a policy.

Do not require every policy in a bundle to map to the bundle's framework.

Custom additions remain valid.

---

# Phase 10 - Requirement Coverage

Implement backend-derived requirement coverage for bundle versions.

Coverage must be based on:

```text
bundle requirement membership
+
selected bundle policy versions
+
accepted policy-to-requirement mappings
```

Never derive authoritative coverage from legacy policy fields such as:

```text
policy.framework
policy.control_family
policy.cci_ids
```

At minimum report:

```text
full
partial
unmapped
```

A requirement is fully covered when the selected policy mappings satisfy the defined full-coverage rules.

Initial simple rule:

```text
at least one:
relationship = implements
coverage = full
```

may produce full coverage.

Any relevant mapping without full implementation may produce partial coverage.

Do not imply mathematical equivalence across different requirements merely because multiple policies map to them.

---

# Phase 11 - Legacy Compliance Metadata

MR !313 currently preserves policy-level compliance/source metadata such as:

```text
framework
control_family
cmmc_level
cis_section
srg_ids
cci_ids
severity
rationale
```

Do not delete this data blindly.

Classify each field as one of:

```text
source metadata
requirement metadata
mapping metadata
policy-native metadata
legacy compatibility metadata
```

Move authoritative compliance meaning into the normalized model.

Where migration is deterministic, backfill normalized requirements/mappings.

Where it is ambiguous, preserve the source metadata and do not guess.

In the UI, legacy/source framework fields may remain under:

```text
Source metadata
Advanced
```

for fidelity.

They must not be presented as the policy's authoritative compliance ownership.

---

# Phase 12 - Deterministic Backfill

Backfill existing data only from authoritative evidence.

Strong deterministic evidence includes:

```text
existing source object mappings
exact STIG V-ID relationships
explicit existing-policy mappings selected during prior imports
exact authoritative imported identifiers
```

Do not create authoritative mappings from:

```text
title similarity
description similarity
CCI overlap alone
control family overlap alone
Nix expression similarity alone
```

Those may later become review candidates.

Migration must be idempotent.

---

# Phase 13 - Compliance Import Adapter Layer

Separate parsing from framework interpretation.

Introduce an abstraction conceptually similar to:

```text
ComplianceImportAdapter
```

Initial implementation:

```text
DisaStigImportAdapter
```

Existing generic XCCDF parsing should remain generic.

Future implementations may include:

```text
GenericXccdfImportAdapter
OscalImportAdapter
CisImportAdapter
CfNativeImportAdapter
```

An adapter must be responsible for determining:

```text
framework identity
framework release identity
requirement canonical identity
requirement hierarchy
source identifier meaning
crosswalk/reference identifiers
implementation hints
```

Do not scatter DISA-specific checks throughout generic parser/reconciliation code.

---

# Phase 14 - Requirement-First STIG Import

Change the foreign STIG import mental model from:

```text
STIG Rule
    ->
Policy
```

to:

```text
STIG Rule
    ->
Normalized Requirement
    ->
Reconcile implementation
        |
        +-- reuse existing policy
        +-- create new policy
        +-- manual
        +-- evidence only
        +-- unimplemented
```

Importing a STIG must first create/reconcile framework and requirement state.

Creating policies is a secondary implementation decision.

---

# Phase 15 - Import Reconciliation Pipeline

STIG import should execute conceptually as:

```text
Upload source artifact
      |
      v
Parse
      |
      v
Identify framework
      |
      v
Reconcile framework release
      |
      v
Reconcile requirement lineages/versions
      |
      v
Reconcile policy implementations
      |
      v
Human review
      |
      v
Atomic commit
```

No authoritative database mutation should occur during preview.

---

# Phase 16 - Framework Release Reconciliation

For a new STIG import, determine:

```text
exact artifact already imported
existing semantic release
new release
same release identity with conflicting content
```

Exact re-import must be idempotent.

Example:

```text
Import Anduril V1R2 SHA ABC
Import Anduril V1R2 SHA ABC again
```

must result in:

```text
0 duplicate framework versions
0 duplicate requirement versions
0 duplicate policies
0 duplicate mappings
0 duplicate bundle versions
```

---

# Phase 17 - Requirement Reconciliation

For every imported requirement classify:

```text
EXISTING_UNCHANGED
EXISTING_CHANGED
NEW_REQUIREMENT
REMOVED_FROM_RELEASE
IDENTITY_CONFLICT
```

For a newer framework release, preview should support summary data such as:

```text
400 requirements

362 unchanged
21 changed
12 new
5 removed
```

Historical requirement versions must never be deleted merely because a later framework release removes the requirement.

---

# Phase 18 - Policy Reconciliation

For each imported requirement, search for reusable implementation candidates before creating a policy.

Use explicit match classes and confidence.

Recommended order:

```text
1. Existing authoritative requirement mapping
2. Mapping inherited from unchanged previous requirement version
3. Exact normalized technical implementation match
4. Strong related compliance mapping
5. Semantic/title similarity
6. No candidate
```

---

# Phase 19 - Existing Authoritative Mapping

Strongest case.

If:

```text
Requirement V-268137
    ->
Policy P17 v3
```

is already authoritative, importing another baseline containing the same requirement should reuse the exact policy version where valid.

Do not create another policy.

---

# Phase 20 - New Framework Release Mapping Inheritance

Given:

```text
V1R1 requirement V-268137
    ->
Policy P17 v3
```

and V1R2 contains the same requirement lineage:

If requirement semantics are unchanged, the existing mapping may be inherited according to deterministic server rules.

If requirement semantics changed, mark the implementation for review.

Do not automatically create a new policy.

---

# Phase 21 - Exact Technical Candidate Matching

Preserve and extend the existing policy-control inference work.

When the imported STIG fix yields normalized enforcement such as:

```text
services.openssh.settings.PermitRootLogin == "no"
```

and an existing policy contains the same canonical enforcement behavior, return it as an exact reusable-policy candidate.

This is a strong candidate, but the user must still be able to review/reject it unless an authoritative mapping already exists.

Do not deduplicate solely using display text.

---

# Phase 22 - Related Mapping Candidates

Cross-framework mappings may provide candidate signals.

Example:

```text
STIG V-xxxxx
references CCI-xxxxx

existing policy maps to related NIST requirement
```

This may produce:

```text
suggested existing implementation
```

but must not automatically create an authoritative policy mapping.

---

# Phase 23 - Fuzzy Similarity

Title/description similarity may be used only for candidate discovery.

It must never silently:

```text
merge policies
reuse policies
create mappings
```

Require explicit review.

---

# Phase 24 - Deduplicate Within One Import

Detect when several STIG requirements appear to share one technical implementation.

Example:

```text
V-111
V-222
V-333
```

all infer the same normalized enforcement.

The UI should allow:

```text
Use one policy for all 3
```

Result:

```text
V-111 ----\
V-222 ----- Policy P17
V-333 ----/
```

Do not create three duplicate policy lineages.

---

# Phase 25 - Manual / Unimplemented Requirements

A requirement may have implementation state such as:

```text
mapped policy
manual
evidence only
unimplemented
opaque source implementation
```

Do not create fake executable policies to represent requirements with no implementation.

Existing opaque source content must still be preserved for round-trip fidelity.

---

# Phase 26 - Preview API

The server must own reconciliation logic.

Do not implement deduplication intelligence in Dioxus.

Return normalized preview state.

Conceptually:

```text
ComplianceImportPreview {
    source
    framework
    framework_version
    framework_reconciliation
    requirement_summary
    requirements[]
}
```

Requirement preview:

```text
RequirementImportPreview {
    source_identity
    normalized_identity

    requirement_state
    previous_requirement_version
    semantic_diff

    inferred_assertions[]

    policy_reconciliation
}
```

Policy reconciliation:

```text
PolicyReconciliation {
    state
    recommended_policy_version_id
    candidates[]
}
```

Candidate:

```text
PolicyCandidate {
    policy_id
    policy_version_id
    policy_name

    match_type
    confidence
    match_reasons[]

    existing_mappings[]
}
```

---

# Phase 27 - Commit Decisions

The client sends explicit user decisions for ambiguous cases.

Conceptually:

```text
RequirementDecision {
    requirement_source_key

    action

    policy_version_id
    relationship
    coverage

    new_policy_definition
    manual_metadata
}
```

The server must treat these as requests, not trusted facts.

---

# Phase 28 - Preview / Commit Trust Boundary

Preserve the !313 TOCTOU protections.

On commit:

1. Revalidate artifact digest.
2. Reparse authoritative uploaded bytes.
3. Recompute framework identity.
4. Recompute framework version identity.
5. Recompute requirement identities.
6. Revalidate user reconciliation decisions.
7. Verify referenced exact policy versions.
8. Acquire required locks.
9. Recheck uniqueness/current DB state.
10. Commit atomically.

The browser must never be authoritative for source identity or reconciliation.

---

# Phase 29 - Atomic Import

A single import may create:

```text
framework
framework version
requirement lineages
requirement versions
bundle lineage/version
bundle requirement membership
policy lineages
policy versions
policy requirement mappings
bundle policy membership
source artifact mappings
audit records
```

All persistence must occur atomically.

Any failure rolls back the complete import.

---

# Phase 30 - Policy APIs

Add backend APIs required by the design.

At minimum provide operations to:

```text
list frameworks
get framework/version
browse/search requirements
get requirement hierarchy
list policy mappings
create mapping on mutable policy version
update mapping on mutable policy version
delete mapping on mutable policy version
get policies mapped to requirement/framework
get requirements mapped to policy
```

Use typed DTOs.

Do not expose raw DB JSON as the API contract.

Pagination/search must be server-side where result sizes may grow large.

---

# Phase 31 - Requirement Search

Requirement search must support at least:

```text
external ID
title
CCI
SRG/reference identifier where applicable
```

Search must be scoped by framework/framework version.

The mapping editor must not download every requirement in a large framework and perform all searching client-side.

---

# Phase 32 - Bundle Coverage APIs

Expose requirement coverage for a bundle version from the server.

The frontend must not recalculate authoritative compliance state from independent client responses.

Return enough information to render:

```text
total
full
partial
unmapped

requirement rows
mapped policies
relationship
coverage
hierarchy/breadcrumb
```

---

# Phase 33 - Policy UI: Pixel-Perfect Design Requirement

Implement the production Policy view according to commit `861fd877`.

For all portions touched by this task, the Dioxus implementation must match the design example pixel-for-pixel.

This includes:

```text
policy cards
policy drawer
policy add/edit modal
modal tabs
mapping rows
mapping editor
framework selector
requirement selector/search
relationship controls
coverage controls
rationale input
provenance display
mapping counts
empty states
source metadata advanced section
bundle usage indicators
```

Do not produce a merely functionally equivalent UI.

Compare the running production UI side-by-side with:

```text
docs/design/CrystalForge/
```

and correct visual deltas.

---

# Phase 34 - Policy Add/Edit Modal

Implement the design's tab structure:

```text
Details
Mappings · N
Enforcement · N
Evidence · N
```

Framework-specific compliance meaning belongs under `Mappings`.

Legacy/source metadata may remain under the advanced source metadata treatment shown in the design.

The modal must allow a policy with zero mappings.

---

# Phase 35 - Mapping Editor

Implement the design interaction shown by `InlineMappingEditor`.

Required fields:

```text
Framework
Requirement
Relationship
Coverage
Mapping rationale
```

Framework selector displays framework/version.

Requirement selector must support server-backed search by:

```text
ID
title
CCI/reference identifiers
```

Selected requirement must display hierarchy context/breadcrumb.

Prevent duplicate mappings according to backend uniqueness semantics.

Relationship choices:

```text
Implements
Supports
Provides evidence for
```

Coverage:

```text
Full
Partial
```

---

# Phase 36 - Mapping Display

Mapped requirements must be grouped by framework.

Each mapping should show at least:

```text
framework/version
external requirement ID
requirement title
relationship
coverage
rationale when present
provenance
```

Imported mappings should visibly identify their source/provenance without dominating the UI.

Suggested mappings, if implemented in this task, must be visually separate from accepted mappings.

---

# Phase 37 - Policy List and Drawer

The Policy view must clearly distinguish:

```text
Mapped to N requirements
Used by N bundles
Used by N systems
```

These are different facts.

Security policy grouping must derive from normalized requirement mappings.

For example, grouping by NIST family means:

```text
policy
 -> mapping
 -> requirement
 -> ancestor family
```

It must not mean:

```text
policy.control_family == "AU"
```

A policy mapped into both AU and SC may legitimately appear in both groups where the design supports multi-group display.

Avoid duplicating policy identity or state merely because it appears in multiple compliance groupings.

---

# Phase 38 - Legacy Grouping Compatibility

The design mock still contains some legacy grouping shortcuts using policy-level:

```text
framework
controlFamily
CCI
```

Do not reproduce those shortcuts as production authority.

Where the design visually groups by:

```text
framework
control family
CCI
CMMC
```

derive those groupings from normalized requirements/mappings.

Legacy fields may remain visible in Advanced / Source Metadata where required for source fidelity.

---

# Phase 39 - Compliance View

Implement the `Requirement coverage` card shown in commit `861fd877`.

It must match the design visually.

It should display:

```text
framework
full count
partial count
unmapped count
```

Expanded view groups requirements by hierarchy and shows:

```text
requirement external ID
title
mapped policies
coverage state
```

The data must come from backend authoritative bundle coverage.

Do not implement the design mock's assumption that all leaf requirements in a framework automatically belong to every bundle.

Use explicit bundle requirement membership.

---

# Phase 40 - Bundle Add/Edit UI

Implement the design behavior where policy selection is divided into:

```text
Mapped to <framework>
Other reusable policies
```

A policy mapped to the bundle's framework appears in the first section.

A policy with no mapping to the framework remains selectable and appears as:

```text
Custom addition
No mapping to <framework>
```

Do not block custom/hybrid bundles.

This UI must match the design example pixel-for-pixel.

---

# Phase 41 - STIG Import UI

Implement the reconciliation step introduced in the design commit before per-control refinement.

The visual design in:

```text
docs/design/CrystalForge/components/ImportStigModal.jsx
```

is authoritative for layout/interaction.

The production implementation must use backend reconciliation data rather than local mock functions.

---

# Phase 42 - STIG Reconciliation Summary

The design currently demonstrates three broad states:

```text
existing implementations reused
ready to create - enforcement inferred
need review - no enforcement inferred
```

Extend the production semantics as required by the normalized backend while preserving the visual intent.

The UI must distinguish at least:

```text
existing authoritative implementation
inherited implementation
strong existing-policy candidate
new implementation ready from inference
needs human review
manual/unimplemented
conflict
```

Do not overload the interface unnecessarily.

Use the design's summary-card and attention-list patterns.

---

# Phase 43 - Refine Only Items Requiring Attention

The normal STIG import path should no longer require manually refining every selected rule.

Default flow:

```text
Import 400 requirements

auto-reconcile most

Review N requiring attention
```

Preserve:

```text
Refine all instead
```

for users who want to inspect every control.

The current design explicitly includes this escape hatch.

---

# Phase 44 - Existing STIG Refine Workflow

Preserve the useful existing workflow:

```text
From the STIG
Enforcement
Evidence
```

and its source details:

```text
official discussion
official check
official fix
V-ID
SRG
CCI
severity
inferred Nix enforcement
evidence configuration
```

Do not regress the inference functionality introduced before this task.

---

# Phase 45 - Policy Reuse UX During Import

When a candidate existing policy is found, show:

```text
policy name
exact version
why it matched
existing framework mappings
match confidence/type
```

Allow:

```text
Use existing
Choose another
Create new
```

Do not silently select the newest revision.

Any selected existing policy must resolve to an exact policy version.

---

# Phase 46 - Shared Implementation UX

When multiple requirements have an exact shared implementation candidate, support:

```text
N requirements appear to share one implementation

[Use one policy for all]
[Review individually]
```

This decision must result in one policy version with multiple requirement mappings.

---

# Phase 47 - Final Import Review

Before commit, provide a summary approximately equivalent to:

```text
Framework
    existing/new lineage
    release

Requirements
    total
    unchanged
    changed
    new
    removed

Implementations
    existing policies reused
    mappings inherited/confirmed
    new policies
    manual/unimplemented

Bundle
    new/existing lineage
    new bundle version

Source
    artifact/provenance
```

Do not state that `N requirements` means `N policies`.

---

# Phase 48 - Source Fidelity

Preserve the complete source-artifact behavior from !313.

Do not discard:

```text
original XCCDF rule IDs
Vulnerability IDs
CCI IDs
SRGs
references
checks
fixes
group hierarchy
opaque XML
source artifact digest
source object mappings
```

Normalized requirements complement source fidelity.

They do not replace it.

---

# Phase 49 - Re-import Behavior

Add explicit tests for all three cases.

## Exact same artifact

Expected:

```text
fully idempotent
no duplicate normalized entities
no duplicate policies
no duplicate mappings
```

## Same claimed release, different content

Expected:

```text
typed conflict
no silent second authoritative release
```

## New framework release

Expected:

```text
reuse framework lineage
create new framework version

reuse requirement lineages
create appropriate requirement versions

reuse valid policy mappings
review changed requirements

create policies only when genuinely new
```

---

# Phase 50 - Requirement Removal

Requirements absent from a new framework release remain present historically.

Do not delete prior requirement versions.

A new bundle/framework version simply does not select them.

Where authoritative metadata indicates removal/deprecation/supersession, preserve it.

---

# Phase 51 - Concurrency

Concurrent imports must not create duplicate:

```text
framework lineages
framework versions
requirement lineages
requirement versions
mappings
bundle versions
policies
```

Use the advisory-lock ordering and transactional patterns established by !313.

Add concurrency tests for identity races.

---

# Phase 52 - Typed Errors

Introduce typed domain errors as needed.

Examples:

```text
FRAMEWORK_RELEASE_CONFLICT
REQUIREMENT_IDENTITY_CONFLICT
REQUIREMENT_VERSION_CONFLICT
POLICY_MAPPING_CONFLICT
POLICY_VERSION_STALE
SOURCE_ARTIFACT_MISMATCH
IMPORT_DECISION_STALE
```

Use the project's established API error patterns.

Do not leak generic SQL/database errors to the UI.

---

# Phase 53 - Authorization

Use existing compliance administration authorization semantics.

Users able to mutate:

```text
framework imports
mappings
policies
bundles
publication state
```

must have the appropriate existing administrative permission.

Read-only users may inspect mappings and coverage.

---

# Phase 54 - Audit Events

Record meaningful events such as:

```text
framework version imported
requirement created
requirement version created
mapping created
mapping removed
mapping inherited
mapping accepted
mapping rejected
existing policy reused
new policy created from imported requirement
```

Preserve actor and source provenance.

---

# Phase 55 - Performance

STIGs may contain hundreds of requirements.

Do not implement:

```text
one SQL query per requirement
one policy scan per requirement
download complete policy library to browser for matching
download complete framework requirement catalog for every search
```

Batch reconciliation queries.

Use indexed canonical identities.

Use normalized/indexed enforcement identity for exact implementation candidate matching where appropriate.

Requirement search should be server-side and bounded.

---

# Phase 56 - Frontend State and Error Handling

The Dioxus UI must correctly handle:

```text
loading
empty
success
partial
conflict
stale decision
API error
authorization error
immutable policy
```

Do not use optimistic UI in places where it can imply an authoritative compliance mapping before persistence succeeds.

If mutation fails, restore the last authoritative server state.

---

# Phase 57 - Pixel-Perfect UI Verification

This is a task acceptance requirement, not optional polish.

For every production UI component changed by this task:

1. Run the design example at commit `861fd877`.
2. Run the production web UI.
3. Navigate both to equivalent states.
4. Compare them side-by-side.
5. Correct visual differences.
6. Repeat for dark/light theme where supported.
7. Repeat at representative desktop widths.

Areas requiring explicit visual comparison:

```text
Policies page
security policy groupings
policy cards
policy drawer mapping content
Add Policy modal
Edit Policy modal
Mappings tab
Add Mapping editor
Edit Mapping editor
Compliance bundle details
Requirement coverage card expanded/collapsed
Bundle Add/Edit policy selector
STIG import reconciliation summary
STIG controls requiring attention
STIG auto-resolved controls
STIG refine flow
final import review
```

Do not declare the task complete because the controls exist.

The touched UI must match the reference design.

---

# Phase 58 - Design Semantics Versus Mock Data

The reference design is authoritative for:

```text
appearance
information hierarchy
interaction
labels
spacing
component arrangement
states
```

It is not authoritative for mock storage semantics.

Specifically, do not implement production logic based on mutable globals such as:

```text
COMPLIANCE_FRAMEWORKS
COMPLIANCE_REQUIREMENTS
POLICY_REQUIREMENT_MAPPINGS
POLICIES
```

and do not preserve mock-only shortcuts such as deriving authoritative grouping from:

```text
policy.framework
policy.controlFamily
```

Production data must come from the backend normalized model.

---

# Phase 59 - API / UI Test Coverage

Add backend tests for:

1. Framework CRUD/identity rules as applicable.
2. Framework release uniqueness.
3. Requirement lineage uniqueness.
4. Requirement hierarchy.
5. Requirement semantic versioning.
6. Policy mapping create/update/delete on mutable draft.
7. Mapping mutation blocked on immutable policy.
8. Derived draft mapping editing.
9. Multiple requirements mapped to one policy.
10. Multiple frameworks mapped to one policy.
11. Multiple families mapped to one policy.
12. Requirement with no policy.
13. Bundle requirement membership.
14. Bundle coverage full.
15. Bundle coverage partial.
16. Bundle coverage unmapped.
17. Custom unmapped policy in framework bundle.
18. Exact STIG re-import.
19. New STIG release.
20. Changed requirement handling.
21. Removed requirement handling.
22. Existing mapping reuse.
23. Inherited unchanged mapping.
24. Changed requirement forcing review.
25. Exact implementation candidate.
26. Fuzzy candidate never auto-accepted.
27. In-import policy deduplication.
28. Preview is mutation-free.
29. Commit rejects stale preview.
30. Concurrent import identity race.
31. Source provenance preservation.
32. Complete rollback on import failure.

Add frontend tests for:

```text
mapping editor validation
mapping duplication prevention
framework requirement search
immutable/read-only mapping state
bundle policy split
coverage rendering
STIG reconciliation routing
attention-only refinement
existing-policy selection
error states
```

---

# Phase 60 - Required End-to-End Acceptance Scenarios

## Scenario A - One policy, multiple NIST families

Create:

```text
Policy:
Require synchronized system time
```

Map:

```text
NIST SC-45
    implements/full

NIST AU-8
    supports/partial
```

Expected:

- one policy lineage
- policy visible through both relevant framework groupings
- mappings visible in Policy UI
- no duplicated policy
- bundle coverage reflects each mapping independently

---

## Scenario B - One policy, multiple frameworks

Create:

```text
Policy:
Disable SSH root login
```

Map:

```text
DISA V-xxxxx
CIS 5.1.8
NIST AC-17
```

Expected:

```text
one policy
three framework mappings
```

---

## Scenario C - Internal policy

Create:

```text
Install internal EDR agent
```

with no compliance mapping.

Expected:

- valid policy
- assignable to systems/bundles
- displayed as unmapped/custom where appropriate
- no framework required

---

## Scenario D - Hybrid bundle

Create:

```text
DISA Anduril STIG + Corporate Hardening
```

containing:

```text
framework-mapped policies
+
internal policies with no DISA mapping
```

Expected:

- both allowed
- UI separates mapped policies and custom additions
- requirement coverage only claims what mappings support

---

## Scenario E - STIG import reuses policy

Existing:

```text
Policy:
Disable SSH root login
```

Import a DISA STIG rule requiring the same canonical enforcement.

Expected:

- importer proposes/reuses existing policy
- no duplicate policy lineage
- new DISA requirement mapping created as appropriate

---

## Scenario F - Multiple STIG requirements, one implementation

Import three requirements with one identical canonical enforcement.

Expected:

- one selected/created policy
- three requirement mappings
- no duplicate technical policies

---

## Scenario G - Requirement with no automatable implementation

Import a STIG requirement with no inferred enforcement.

Expected:

- normalized requirement exists
- requirement remains in bundle baseline
- user can mark manual/unimplemented
- no fake executable policy is required

---

## Scenario H - New STIG revision

Existing:

```text
Anduril STIG V1R1
```

Import:

```text
Anduril STIG V1R2
```

Expected:

```text
same framework lineage
new framework version

unchanged requirement lineages reused
new requirement versions as appropriate
new requirements created
removed requirements preserved historically

existing policy work reused
changed requirements reviewed
new policies created only when genuinely necessary
```

---

# Phase 61 - Verification

At minimum, before task completion run:

```text
nix build .#web-ui
nix build .#server
nix flake check --keep-going
```

Run all relevant:

```text
cf-server library tests
compliance interchange DB tests
policy resolver tests
publication lifecycle tests
deletion lifecycle tests
web-ui tests
XCCDF parser/writer tests
```

Also require:

```text
cargo fmt --all --check
git diff --check
```

No production:

```text
println!
dbg!
eprintln!
```

Do not commit local STIG fixture archives or generated files unless explicitly required by the repository test fixture policy.

---

# Phase 62 - Scope Control

This task is already large.

Do not expand it into unrelated work.

Do not redesign:

```text
general deployment policy execution
evaluation queue behavior
builder behavior
deployment strategy
notifications
sessions
CVE scanning
```

unless a direct regression from this implementation requires a minimal fix.

Do not broadly refactor !313 code merely because it can now be written differently.

Preserve stable architecture unless this task specifically requires a change.

---

# Definition of Done

This task is complete only when all of the following are true:

- policies are framework-neutral in the authoritative backend model
- normalized frameworks and framework versions exist
- normalized requirement lineages and versions exist
- policy-to-requirement mappings are first-class and version-safe
- a policy can map to multiple requirements and frameworks
- bundle versions have explicit requirement membership
- requirement coverage is derived from normalized mappings
- STIG imports reconcile requirements before policies
- existing policies are reused where safe
- duplicate policy creation is prevented/reduced through reconciliation
- manual/unimplemented requirements do not require fake executable policies
- exact re-import is idempotent
- newer framework releases reuse prior requirement/policy work safely
- source provenance and XCCDF fidelity remain preserved
- preview/commit remains TOCTOU-safe and atomic
- the production Policy UI implements the mapping workflow
- the production Compliance UI implements requirement coverage
- the production Bundle UI distinguishes mapped policies from custom additions
- the production STIG import implements reconciliation-before-refinement
- all UI touched by this task matches commit `861fd877` pixel-for-pixel
- legacy policy framework/family metadata is not authoritative
- all required automated tests pass
- `nix build .#web-ui` passes
- `nix build .#server` passes
- `nix flake check --keep-going` passes

---

# Final Architecture

The finished system should support:

```text
                         DISA V-111
                            ^
                            |
                            |
NIST SC-45 <--------- Policy P17 ---------> CIS 2.1.1
                            |
                            |
                         DISA V-222
```

while a bundle independently defines:

```text
Bundle Version

Requirement baseline:
    V-111
    V-222
    V-333

Selected implementations:
    P17
    P24
```

A STIG import should therefore ask:

```text
What requirements does this authoritative source define?

Which of those requirements already exist?

Which policies already implement them?

Which existing policies can safely be reused?

Which requirements genuinely require new implementation work?
```

It must no longer assume:

```text
one imported STIG rule = one new Crystal Forge policy
```

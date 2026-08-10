---
id: TASK-412
title: Implement CF-XCCDF bundle and policy interchange and design updates
status: In Progress
assignee:
  - '@gpt-5.6-terra'
created_date: '2026-08-01 01:04'
updated_date: '2026-08-10 01:25'
labels:
  - design
  - frontend
  - web-ui
  - backend
  - api
  - database
  - compliance
  - policies
  - scanning
  - security
  - xml
  - xccdf
  - testing
dependencies: []
references:
  - 'commit:5410121ebf4e5eebd64b06d3a78e82d052329e50'
  - docs/design/CrystalForge/components/ComplianceView.jsx
  - docs/design/CrystalForge/components/PoliciesView.jsx
  - docs/design/CrystalForge/components/ScanningView.jsx
  - docs/design/CrystalForge/components/Shell.jsx
  - docs/design/CrystalForge/styles.css
  - >-
    docs/design/CrystalForge/docs/crystal-forge-xccdf-interchange-profile-v0.1.md
  - packages/default/crates/cf-server/src/models/deployment_policies.rs
  - packages/default/crates/cf-server/src/queries/compliance.rs
  - packages/default/crates/cf-server/src/api/models.rs
  - packages/web-ui/src/views/compliance.rs
  - packages/web-ui/src/views/policies.rs
  - packages/web-ui/src/views/scanning.rs
  - packages/web-ui/src/export/mod.rs
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/313'
modified_files:
  - migrations/
  - packages/default/crates/cf-server/src/api/
  - packages/default/crates/cf-server/src/models/
  - packages/default/crates/cf-server/src/queries/
  - packages/default/crates/cf-server/src/compliance/
  - packages/default/crates/cf-server/resources/schema/
  - packages/web-ui/src/api/
  - packages/web-ui/src/components/
  - packages/web-ui/src/views/
  - packages/web-ui/src/export/
  - packages/web-ui/assets/
  - >-
    docs/design/CrystalForge/docs/crystal-forge-xccdf-interchange-profile-v0.1.md
priority: high
type: feature
ordinal: 405500
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Implement the compliance, policy interchange, and scanning changes from design commit `5410121ebf4e5eebd64b06d3a78e82d052329e50` in the real Crystal Forge server and Dioxus/WASM application.

This task must implement the behavior behind the design. It must not copy the design prototype's in-browser XML parsing, string-built XML, mock-array mutation, or lossy policy conversion into production code.

The task has four connected goals:

1. Implement CF-XCCDF v0.1 as a server-side import and export format for compliance bundles and policies.
2. Add durable versioning, publication, trust, source preservation, and bundle-assignment semantics.
3. Implement the compliance and policy import/export user flows shown in the design.
4. Apply the exact scanning and shared-menu UI delta from the reference commit.

The final result must support this workflow:

```text
Import a foreign STIG or CF-XCCDF benchmark
    -> inspect benchmark metadata and requirements
    -> select a profile and rules
    -> map rules to existing policies, create unbound draft policies, or preserve unsupported rules
    -> create a draft bundle
    -> review and trust executable content
    -> publish an immutable bundle version when ready
    -> assign the bundle to an environment or system
    -> enforce the bundle baseline by default
    -> add or exclude policies through an assignment overlay
    -> export the bundle as XCCDF
    -> reimport it into another Crystal Forge instance without losing supported policy semantics
```

### Product semantics

#### Policies remain the executable unit

A deployment policy remains Crystal Forge's primary unit for enforcement and evidence collection.

A policy can run during:

- Nix evaluation;
- post-build processing;
- pre-deployment checks;
- deployment orchestration; or
- continuous assessment.

The current executable policy types must round-trip without loss:

- `require_cf_agent`;
- `require_packages`;
- `custom_check`, including legacy single-expression and multi-rule `all` or `any` forms;
- `require_cve_check`;
- `time_window`;
- `require_approvals`;
- `canary_rollout`; and
- `cve_threshold`.

Foreign XCCDF rules that Crystal Forge cannot execute must still be stored and displayed. They must use an explicit non-executable state such as manual, external, unbound, or opaque. The importer must not invent a Nix expression from prose.

#### A compliance bundle is a baseline

A bundle version contains an ordered set of policy versions.

Assigning a bundle must enforce every selected policy in the baseline by default. An assignment can then:

- exclude baseline policies;
- add ad hoc policies;
- override supported values;
- run in enforce or report-only mode; and
- coexist with direct environment and system policies.

The effective set is:

```text
effective policies =
    bundle baseline
    - assignment exclusions
    + assignment additions
    + direct environment policies
    + direct system policies
```

Direct system policy choices have higher specificity than environment choices. Environment choices have higher specificity than a bundle baseline. If two sources select different versions of the same policy lineage at the same specificity, Crystal Forge must report a conflict. It must not silently choose the newest version.

#### Draft and published versions

Draft policy and bundle versions are mutable.

Published policy and bundle versions are immutable. Editing a published version must create a new draft derived from that version.

Publishing a bundle must create a stable snapshot of:

- bundle metadata;
- ordered membership;
- exact policy-version references;
- standard compliance metadata;
- semantic digests; and
- source provenance.

A published bundle can include only immutable policy versions. The publish operation may atomically publish included draft policy versions as part of the same transaction.

#### Import is not activation

Imported executable content is untrusted by default.

Importing a file must not automatically:

- enable a policy;
- assign a bundle;
- evaluate a Nix expression;
- add a flake input;
- import a NixOS module;
- schedule a deployment;
- grant approval authority; or
- alter an existing environment or system policy set.

The user must complete an explicit review or trust action before activation. A local publisher-trust policy can later permit automatic trust for a verified signature, but signature verification alone must not activate content.

### Exact design delta

The reference commit changed existing design screens. Do not treat all features visible in those files as new scope.

The required design delta is:

- Add a shared `IOMenu` component.
- Replace separate compliance import/export buttons with the shared menu.
- Add CF-XCCDF bundle export.
- Add CF-XCCDF and foreign XCCDF bundle import.
- Add policy import and export for JSON and TOML.
- Add policy export selection mode and per-policy export.
- Add a default `Deployed` tab to Scanning.
- Mark the latest commit for each flake with the existing gold star treatment.
- Add the related menu and selection styling.

Existing queue tables, activity feeds, schedule controls, bulk helpers, global search, notification behavior, classification banners, and evidence drawers must not be reimplemented unless this task must modify them to support the delta above.

## Goals

- Provide a stable public interchange path without creating a competing top-level benchmark standard.
- Preserve useful standard XCCDF content for non-Crystal-Forge tools.
- Preserve exact Crystal Forge policy behavior for a Crystal Forge-to-Crystal Forge round trip.
- Let users import official or third-party STIG/XCCDF content as visible draft requirements even when the checks are not executable.
- Make bundle assignment flexible while enforcing the baseline by default.
- Keep imports safe, reviewable, transactional, and auditable.
- Keep the browser UI thin. Perform parsing, validation, reconciliation, digest generation, and canonical export on the server.

## Out of scope

- Claiming that Crystal Forge-authored content is an official DISA STIG.
- Generic SCAP scanner execution of Crystal Forge Nix checks.
- Automatic translation of prose, shell commands, OVAL, or OCIL into Nix expressions.
- Full SCAP source data stream generation.
- Automatic installation of modules or flake inputs referenced by imported policies.
- Byte-for-byte regeneration of a modified foreign XCCDF document.
- Automatic bundle assignment during import.
- XCCDF `TestResult` export in this task. Existing evidence exports remain available.
- Implementing a fleet-wide rescan endpoint only because the design contains a Rescan all button.

## Current-state gaps

The existing implementation has these gaps that this task must address:

- Compliance bundles store mutable metadata and local policy UUIDs only.
- Bundle membership and environment assignment are separate, but there is no first-class bundle assignment overlay.
- Policy definitions do not have portable lineage IDs, immutable version IDs, publication state, or semantic digests.
- The compliance evaluator supports only a subset of policy types.
- Compliance statuses do not represent `notchecked`, `notapplicable`, or evaluator errors.
- The current STIG modal uses sample data and does not persist parsed XCCDF.
- The current Policies view can fall back to mock policies during server errors. Import or export must never operate on mock data.
- The design prototype parses XML in the browser and builds XML through string concatenation. Production code must not use that approach.
- The design prototype exports only one simplified custom expression. Production export must preserve every supported policy type and multi-rule structure.
- The design prototype matches or creates objects by slugs and names. Production reconciliation must use portable identities and digests.
- The current scanning queue endpoint is not sufficient to identify all deployed configurations or the latest commit per flake if the result is paginated.

## Architecture

### 1. Canonical server model

Use the server's typed Rust structures as the canonical representation.

XCCDF, JSON, and TOML are adapters around this model. Do not shape the database around XML element layout.

Add typed canonical structures for:

- policy lineage;
- policy version;
- bundle lineage;
- bundle version;
- ordered bundle membership;
- standard compliance metadata;
- source identities and framework mappings;
- policy execution phase;
- policy implementation state;
- dependency declarations;
- source artifacts;
- source-object mappings;
- assignment overlays; and
- import fidelity and trust state.

### 2. Policy implementation states

Support these implementation states for imported content:

- `native`: Crystal Forge can execute the policy.
- `manual`: a user must provide or attest to evidence.
- `external`: the rule references a supported external check system.
- `unbound`: the requirement exists but has no implementation.
- `opaque`: Crystal Forge preserves the rule but cannot model its check semantics.

Only `native`, supported `external`, and explicitly supported `manual` policies can be activated.

An enforce-mode bundle assignment must fail validation when it contains unresolved `unbound` or `opaque` policies unless the assignment excludes them. A report-only assignment can include them and must show `notchecked`.

### 3. Standard compliance metadata

Store standard metadata separately from executable policy configuration. At minimum support:

- source benchmark ID;
- source profile ID;
- source group ID;
- source rule ID;
- STIG or vulnerability ID;
- legacy IDs;
- CCI IDs;
- SRG IDs;
- other `ident` values with their systems;
- severity;
- title;
- discussion or description;
- rationale;
- check text;
- fix text;
- references;
- version;
- platform applicability;
- source check-system URI;
- original rule order; and
- preserved unknown XML.

Severity and enforcement strictness must remain independent.

### 4. Versioning and identities

Use two portable UUID identities for each logical object:

- lineage ID: stable across versions;
- version ID: unique for an exact version.

Local database primary keys may use the same UUIDs, but exports must not depend on installation-specific integer IDs or names.

Generate XCCDF IDs from version UUIDs with a deterministic NCName-safe form, for example:

```text
xccdf_org.crystalforge_benchmark_<bundle-version-uuid-without-hyphens>
xccdf_org.crystalforge_profile_<bundle-version-uuid-without-hyphens>_baseline
xccdf_org.crystalforge_rule_<policy-version-uuid-without-hyphens>
```

Imported foreign IDs must be preserved separately. Do not rewrite official IDs into Crystal Forge IDs and discard the originals.

### 5. Canonical digest

Freeze the first semantic canonicalization format as `cf-model-json-1`.

To calculate a digest:

1. Convert the typed object into its canonical semantic DTO.
2. Exclude local database IDs, timestamps that do not affect behavior, trust state, and assignment state.
3. Sort object keys lexicographically.
4. Preserve ordered arrays when order is semantic or part of the presentation contract.
5. Sort set-like arrays by their normalized value.
6. Encode UTF-8 JSON without insignificant whitespace.
7. Calculate SHA-256 over those bytes.

Store:

- `semantic_digest`;
- `digest_algorithm = sha-256`; and
- `canonicalization_version = cf-model-json-1`.

The digest must cover executable configuration, policy metadata that affects exported meaning, bundle membership, order, and portable identities.

### 6. CF-XCCDF namespace and schemas

Freeze these v0.1 identifiers:

```text
XCCDF namespace: http://checklists.nist.gov/xccdf/1.2
CF extension namespace: urn:crystal-forge:xccdf:1
CF check system: urn:crystal-forge:check-system:policy:1
CF Nix fix system: urn:crystal-forge:fix-system:nix:1
```

Add and vendor:

- the XCCDF 1.2 schema and required imported schemas;
- `cf-xccdf-1.xsd` for the Crystal Forge extension;
- schema provenance and license notices; and
- valid and invalid fixtures.

The Nix build and test suite must not fetch schemas from the network.

### 7. Server-side XML processing

Use a server-side XML parser and writer that supports the required security controls and the Nix build.

The implementation must:

- disable DTD processing;
- disable external entities;
- disable network retrieval;
- enforce document size, depth, attribute, and text limits;
- validate XCCDF structure;
- validate recognized Crystal Forge extension content;
- preserve unknown content;
- write XML through an XML writer, not string concatenation;
- escape text and attributes correctly;
- handle a literal `]]>` in Nix expressions; and
- return structured parse and validation diagnostics.

Do not parse XCCDF with browser `DOMParser` as the authoritative path.

### 8. XCCDF export mapping

One bundle version exports as one XCCDF 1.2 `Benchmark`.

The benchmark must contain:

- standard `id`, `status`, `title`, `description`, and `version` fields;
- `cf:bundle` metadata with lineage ID, version ID, publication state, digest, framework, layer, and owner;
- one baseline `Profile` that explicitly selects every baseline policy rule;
- one `Rule` per included policy version;
- standard severity, identifiers, references, check text, fix text, and applicability data;
- one Crystal Forge check for every native policy;
- preserved standard or opaque checks when available; and
- enough human-readable content for a generic viewer.

Encode all current native policy types with typed Crystal Forge XML elements as defined by CF-XCCDF v0.1.

A legacy single-expression custom check must export as one nested `cf:rule`.

A multi-rule custom check must preserve:

- `all` or `any` mode;
- rule order;
- exact Nix expression text, except XML line-ending normalization;
- field names;
- per-rule descriptions; and
- per-rule strictness.

Local environment and system UUIDs must not be exported as portable bundle assignments. The export can include non-authoritative deployment hints by name or tag, but an importer must never auto-assign from those hints.

### 9. XCCDF import classification

Classify each input as:

- CF-native exact;
- CF-native with unsupported extension content;
- foreign XCCDF or STIG;
- invalid XCCDF; or
- unsupported package.

Report one fidelity state:

- `native_exact`;
- `normalized_complete`;
- `preserved_opaque`; or
- `degraded`.

A degraded import must list every known loss before commit.

For a CF-native import:

1. Match by portable version identity.
2. Verify the semantic digest.
3. Reuse an identical immutable version.
4. Create a new version when lineage matches and version identity differs.
5. Reject or require explicit conflict resolution when an identity matches but the digest differs.
6. Never match only by name or title.

For a foreign XCCDF import:

- preserve benchmark, profile, group, rule, value, check, fix, identifier, reference, and platform data;
- let the user select the source profile;
- create one policy record per selected rule;
- classify each policy as native, manual, external, unbound, or opaque;
- default new policies and the new bundle to draft and untrusted;
- preserve unknown XML and original bytes; and
- never generate a Nix implementation from prose.

### 10. Source preservation

Persist each committed import as a source artifact with:

- original bytes;
- original filename;
- media type;
- SHA-256 digest;
- import time;
- importing user;
- parser version;
- detected XCCDF version;
- package context;
- signature details;
- trust decision; and
- source-object mappings.

If the imported foreign source is unchanged, allow an original-byte re-export.

### 11. Policy JSON and TOML interchange

Implement a canonical policy document and policy-set document.

Canonical JSON example:

```json
{
  "schema": "urn:crystal-forge:policy-set:1",
  "policies": [
    {
      "lineage_id": "11111111-1111-1111-1111-111111111111",
      "version_id": "22222222-2222-2222-2222-222222222222",
      "version": "1.0.0",
      "publication_state": "draft",
      "name": "firewall-enabled",
      "description": "The NixOS firewall must be enabled.",
      "policy_type": "custom_check",
      "execution_phase": "nix-evaluation",
      "config": {
        "mode": "all",
        "rules": [
          {
            "field_name": "firewallEnabled",
            "description": "The evaluated firewall option is true.",
            "expression": "cfg.config.networking.firewall.enable",
            "strict": true
          }
        ]
      }
    }
  ]
}
```

The TOML form must represent the same typed fields.

Canonical exports must support every current native policy type. They must not flatten policies to a single `description`, `expression`, and `strict` object.

For compatibility, the importer may accept the existing bare API object and the design prototype's simplified single-custom-check shape. Compatibility inputs must normalize to the canonical model before validation.

Import must preserve the source's enabled default, but local imported policies must remain disabled and untrusted until explicitly activated.

### 12. Bundle assignments and overlays

Add first-class bundle assignment records for environment and system scopes.

Each assignment must include:

- exact bundle version ID;
- scope type;
- scope ID;
- enforcement mode, default `enforce`;
- excluded policy version IDs;
- added policy version IDs;
- supported value overrides;
- provenance;
- effective-set digest;
- creation and update user; and
- timestamps.

The effective-policy resolver must use the same logic for:

- Nix evaluation;
- build and deployment gates;
- compliance rollups;
- evidence views;
- assignment previews; and
- exports of derived effective benchmarks.

Do not implement separate policy-selection rules in the frontend and backend.

### 13. Compliance evidence semantics

Expand compliance result states to support:

- `pass`;
- `warn`;
- `fail`;
- `waiver`;
- `not_checked`;
- `not_applicable`;
- `error`; and
- `informational` when needed.

Do not treat a disabled or unsupported policy as a warning that looks evaluated.

The score denominator must include only controls with a meaningful evaluated result. The response must expose both total selected controls and evaluated controls.

Use the latest applicable evidence for each policy type:

- Nix policies: persisted per-policy results from the relevant evaluation and deployed target.
- CVE policies: the latest completed scan for the relevant derivation.
- Time-window policies: the latest applicable deployment decision or `not_checked` when no decision exists in the selected assessment context.
- Approval policies: approval records for the relevant deployment decision.
- Canary policies: rollout state and health observations for the relevant deployment.
- Manual policies: accepted manual evidence or `not_checked`.
- Unbound or opaque policies: `not_checked`.

A waiver must preserve the observed result. It must not convert a failed control to a pass.

## Database changes

Use additive migrations and preserve existing IDs where possible.

### Policy versioning

Keep `deployment_policies.id` as the policy lineage ID to reduce migration risk.

Add a `deployment_policy_versions` table with at least:

- `id UUID PRIMARY KEY` as version ID;
- `policy_id UUID NOT NULL` as lineage ID;
- `version TEXT NOT NULL`;
- `publication_state TEXT NOT NULL`;
- `published_at TIMESTAMPTZ`;
- `name TEXT NOT NULL`;
- `description TEXT`;
- `policy_type TEXT NOT NULL`;
- `implementation_state TEXT NOT NULL`;
- `execution_phase TEXT NOT NULL`;
- `config JSONB NOT NULL`;
- `compliance_metadata JSONB NOT NULL`;
- `dependencies JSONB NOT NULL`;
- `semantic_digest TEXT NOT NULL`;
- `canonicalization_version TEXT NOT NULL`;
- `source_artifact_id UUID`;
- `opaque_xml TEXT`;
- `derived_from_version_id UUID`;
- `created_by UUID`;
- `created_at TIMESTAMPTZ NOT NULL`; and
- a uniqueness constraint for portable version identity.

Add current draft and current published version pointers to `deployment_policies` or provide equivalent deterministic queries.

Backfill every existing policy as an initial draft version. Preserve the exact current config and enabled state.

### Bundle versioning

Keep `compliance_bundles.id` as the bundle lineage ID.

Add `compliance_bundle_versions` with at least:

- `id UUID PRIMARY KEY` as version ID;
- `bundle_id UUID NOT NULL` as lineage ID;
- `version TEXT NOT NULL`;
- `publication_state TEXT NOT NULL`;
- `published_at TIMESTAMPTZ`;
- `name TEXT NOT NULL`;
- `framework TEXT NOT NULL`;
- `framework_version TEXT`;
- `description TEXT`;
- `layer TEXT NOT NULL`;
- `owner TEXT NOT NULL`;
- `semantic_digest TEXT NOT NULL`;
- `canonicalization_version TEXT NOT NULL`;
- `source_artifact_id UUID`;
- `derived_from_version_id UUID`;
- `created_by UUID`;
- `created_at TIMESTAMPTZ NOT NULL`; and
- a uniqueness constraint for portable version identity.

Add `compliance_bundle_version_policies` with:

- bundle version ID;
- policy version ID;
- explicit order;
- selected state; and
- uniqueness constraints that prevent duplicate membership.

Backfill every existing bundle and its membership as an initial draft version.

### Assignments

Add:

- `compliance_bundle_assignments`;
- `compliance_assignment_exclusions`;
- `compliance_assignment_additions`; and
- `compliance_assignment_value_overrides`.

Use foreign keys and transactionally maintain effective-set digests.

Migrate existing `required_envs` bundle relationships into explicit environment bundle assignments. Preserve current behavior, but do not export those local assignments as part of the portable benchmark.

### Source artifacts

Add:

- `compliance_source_artifacts`; and
- `compliance_source_object_mappings`.

Store source bytes in the existing artifact/object-storage pattern when available. Do not add large unbounded XML or ZIP blobs directly to hot relational rows if the repository already has an artifact storage abstraction.

### Trust and audit

Add or reuse fields for:

- trust state;
- trusted by;
- trusted at;
- publisher identity;
- signature status;
- import actor;
- publish actor; and
- conflict-resolution decisions.

Every import, publish, trust, assignment, exclusion, and addition must produce an audit event.

## API requirements

Use admin authorization for import, trust, publish, assignment, and mutation endpoints. Read and export access must follow the existing compliance read permissions unless export can reveal restricted policy content.

### Bundle and version APIs

Add or extend endpoints for:

```text
GET    /api/v1/compliance/bundles
POST   /api/v1/compliance/bundles
GET    /api/v1/compliance/bundles/:bundle_id
POST   /api/v1/compliance/bundles/:bundle_id/drafts
PUT    /api/v1/compliance/bundle-versions/:version_id
POST   /api/v1/compliance/bundle-versions/:version_id/publish
GET    /api/v1/compliance/bundle-versions/:version_id/xccdf
DELETE /api/v1/compliance/bundle-versions/:version_id
```

Rules:

- A published version cannot be updated or deleted through normal mutation endpoints.
- A delete can remove an unused draft only.
- Publishing must validate membership, policy readiness, identities, digests, and schema export in one transaction.
- Export must return `application/xml` with a safe `Content-Disposition` filename.

### XCCDF preview and import APIs

Use a stateless two-call flow. The browser retains the selected file and resubmits it for commit.

```text
POST /api/v1/compliance/xccdf/preview
POST /api/v1/compliance/xccdf/import
```

`preview` accepts multipart XML or ZIP and returns:

- source SHA-256;
- detected document type;
- benchmark metadata;
- available profiles;
- selected rules;
- identifiers and severity counts;
- CF-native or foreign classification;
- signature and trust information;
- fidelity state;
- per-rule implementation state;
- exact matches;
- version conflicts;
- unsupported content;
- dependency declarations;
- validation warnings; and
- blocking errors.

`import` accepts the same file plus an import plan:

- expected source SHA-256;
- selected benchmark and profile;
- selected rule IDs;
- per-rule action: reuse, import native, create manual, create unbound, preserve opaque, map to existing, or exclude;
- conflict resolution;
- bundle name and version;
- publication state, which must default to draft;
- trust decision;
- optional post-import assignment request; and
- optional assignment overlay.

The server must reparse the file, verify the digest, validate the plan against the reparsed document, and commit all changes in one transaction. A partial import must not remain after failure.

### Policy interchange APIs

Add:

```text
POST /api/v1/policies/interchange/preview
POST /api/v1/policies/interchange/import
POST /api/v1/policies/interchange/export
GET  /api/v1/policy-versions/:version_id/export?format=json|toml
POST /api/v1/policy-versions/:version_id/publish
POST /api/v1/policies/:policy_id/drafts
```

The export endpoint accepts exact policy-version IDs and a format. It returns one canonical policy or policy-set document.

The preview and import endpoints use the same digest-checked two-call pattern as XCCDF import.

### Assignment APIs

Add:

```text
GET    /api/v1/compliance/assignments
POST   /api/v1/compliance/assignments
GET    /api/v1/compliance/assignments/:id/effective-policies
PUT    /api/v1/compliance/assignments/:id
DELETE /api/v1/compliance/assignments/:id
```

The effective-policy response must include provenance for each policy:

- bundle baseline;
- bundle addition;
- environment direct;
- system direct;
- excluded; or
- conflict.

### Error contract

Return structured errors with:

- stable error code;
- human summary;
- field or source location when available;
- XML line and column when available;
- rule or object identity;
- whether the error blocks import or publication; and
- remediation text.

Do not return raw parser panics or database errors to the UI.

## UI requirements

### Shared Import / Export menu

Create a reusable Dioxus component instead of duplicating menu logic.

The component must support:

- action items and separators;
- disabled items with reasons;
- danger items;
- keyboard navigation;
- `Escape` to close;
- outside-click close;
- focus return to the trigger;
- correct ARIA menu roles;
- viewport-aware placement; and
- cleanup of event handlers.

Use it in Compliance and Policies.

### Compliance page actions

Replace the separate header actions with one `Import / Export` menu.

Menu entries:

- `Import STIG or XCCDF (.xml/.zip)` for admins;
- `Import Crystal Forge bundle (.xml)` for admins;
- separator;
- `Export this bundle (XCCDF .xml)` when a bundle version is selected;
- `Export evidence report...` using the existing evidence export flow.

Do not show mutation actions to users who do not have permission.

### Import STIG or foreign XCCDF flow

Replace sample-only behavior with the server preview and import APIs.

Use these steps:

1. **Upload**
   - Accept `.xml` and bounded `.zip` inputs.
   - Show file name, size, and SHA-256 after preview.
   - Show parse, schema, signature, and package errors.

2. **Benchmark and profile**
   - Show benchmark title, version, publisher, status, and platform.
   - Let the user select a profile when more than one profile exists.
   - Show rule counts by severity and implementation state.

3. **Rules and implementation**
   - Let the user include or exclude rules.
   - Support search and filters for severity, group, identifier, and implementation state.
   - Show title, discussion, check, fix, CCI, SRG, Rule ID, STIG ID, and legacy IDs.
   - Let the user map a rule to an existing policy version.
   - Let the user create an unbound, manual, or opaque draft policy when no native implementation exists.
   - Never offer an automatically generated Nix expression as if it were authoritative.

4. **Bundle and trust**
   - Set bundle name, version, framework, layer, and owner.
   - Default to a draft bundle.
   - Show fidelity and data-loss warnings.
   - Show executable Nix expressions and dependency declarations before trust.
   - Let the user import without assigning.
   - Offer an explicit `Import, trust, and assign` path only after the user reviews the content and chooses a target scope.

5. **Done**
   - Show counts for reused, created, unbound, opaque, excluded, and conflicted policies.
   - Link to the new bundle and imported policies.
   - Show whether the bundle is draft, trusted, published, and assigned.

### Import Crystal Forge bundle flow

Use the same upload component, but optimize the preview for CF-native content.

Show:

- bundle lineage and version identities;
- publication state;
- content digest verification;
- signature status;
- exact existing matches;
- new versions;
- identity conflicts;
- unsupported extension content;
- policy dependencies;
- source enabled defaults; and
- local trust state.

The user must choose a resolution for every blocking identity conflict.

A matching published identity with a different digest must never be silently replaced.

### Bundle editor and publication

Update the bundle editor to show:

- draft or published state;
- lineage and version;
- derived-from version;
- ordered policies;
- implementation readiness;
- trust state;
- publication validation errors;
- current assignments; and
- assignment overlays.

Published versions must be read-only. The primary edit action must create a new draft.

Add a publish action with a confirmation that shows the exact policy versions that will be frozen.

### Bundle assignment editor

Provide a clear baseline and overlay editor:

- baseline policies from the selected bundle version;
- excluded policies;
- added policies;
- direct environment policies;
- direct system policies;
- conflicts;
- unresolved policies; and
- effective policy count.

Default enforcement mode to `Enforce`.

Show a preview of the effective set before save.

### Compliance catalog and evidence

Keep the current catalog, score strip, systems matrix, and evidence drawer design.

Update them to use bundle versions and effective assignment semantics.

Show badges for:

- draft;
- published;
- trusted or untrusted;
- native exact, normalized complete, preserved opaque, or degraded; and
- assigned or unassigned.

Show `Not checked`, `Not applicable`, and `Error` separately from warnings.

### Policies page

Implement the design's import/export controls against real APIs.

Requirements:

- Add the shared `Import / Export` menu.
- Add `Import policies...`.
- Add `Select policies to export...`.
- Add `Export all custom policies`.
- Add per-policy export in the detail drawer.
- Add selection mode with a visible checkbox treatment.
- Preserve search and category filters while selecting.
- Clear selection when the user cancels, changes route, or completes export.
- Export exact selected policy versions, not current mutable names.
- Let the user select JSON or TOML.
- Preview imports before commit.
- Show name collisions separately from identity conflicts.
- Default imported policies to draft, disabled, and untrusted.
- Reload policies from the API after import.

Remove or disable the mock fallback for policy management in production. A network or server error must show an error state. It must not show or export mock policy data.

### Scanning page

Apply only the scanning delta from commit `5410121e`:

- Add a `Deployed` tab.
- Make `Deployed` the default tab.
- Keep `Active & Recent` and `All configs`.
- Show the existing gold star before the commit hash for every row that belongs to the latest known commit of its flake.

The Deployed tab must use a complete server query for deployed configurations. It must not filter the first page of the active queue in the browser.

Latest-per-flake status must come from a server field or a query that has complete flake history. It must not infer a latest commit from a paginated subset.

Keep current loading, error, empty, schedule, activity, and expansion behavior.

### Styling

Implement the reference menu and policy-selection styling with existing theme tokens.

Do not hardcode design-only colors where an existing token or status component exists.

Ensure:

- menu layering works with drawers and fixed banners;
- the menu does not render under the page header or tray;
- selected policy cards remain readable in light and dark themes;
- keyboard focus is visible; and
- narrow-screen layouts do not overflow.

## Security and resource limits

Define server constants and tests for at least:

- maximum XML upload size;
- maximum ZIP upload size;
- maximum expanded archive size;
- maximum archive file count;
- maximum XML depth;
- maximum attributes per element;
- maximum text-node length;
- maximum rule count;
- maximum profile count;
- maximum policy expression length; and
- maximum preserved opaque XML size per object.

Reject:

- DTDs;
- external entities;
- network schema references;
- path traversal in archives;
- symlinks in archives;
- nested archives beyond the supported limit;
- ZIP bombs;
- duplicate benchmark, rule, profile, value, or portable version IDs that create ambiguity;
- invalid UUID identities in CF-native metadata;
- unsupported CF extension versions when exact import is requested; and
- imports that exceed database or request limits.

Imported Nix expressions must use the existing bounded evaluation path. Import must not create a new evaluator or bypass current time, output, process, or resource limits.

## Implementation order

Implement in this order so UI work does not depend on mock behavior:

1. Freeze CF-XCCDF v0.1 namespace, canonical digest, and extension XSD.
2. Add migrations and backfill current policies and bundles as initial draft versions.
3. Add canonical Rust models, versioning services, publication logic, and effective-policy resolution.
4. Add secure XCCDF parser, writer, validator, source preservation, and round-trip fixtures.
5. Add policy JSON/TOML codecs and APIs.
6. Add bundle XCCDF preview, import, export, and conflict APIs.
7. Extend compliance evidence and result states.
8. Add assignment APIs and migrate current required-environment relationships.
9. Implement the shared menu and policy UI.
10. Implement compliance import, export, publish, and assignment UI.
11. Apply the scanning Deployed tab and latest-per-flake star.
12. Add compatibility, security, database, API, and web-ui tests.
13. Update the CF-XCCDF design document with any implementation decisions and tested compatibility versions.

## Test requirements

### Unit tests

Add tests for:

- canonical digest stability;
- digest changes for every semantic field;
- draft mutation;
- published immutability;
- publish transaction rollback;
- effective-policy precedence and deduplication;
- same-lineage version conflicts;
- every native policy XML encoding and decoding;
- legacy single custom-check normalization;
- multi-rule `all` and `any` round trips;
- exact Nix expression and field-name preservation;
- literal `]]>` handling;
- absent versus zero optional values;
- unknown CF policy preservation;
- foreign unknown XML preservation;
- profile selection;
- ordered bundle membership;
- JSON and TOML single-policy and policy-set round trips;
- compatibility import of the old simplified policy shape;
- result-state and waiver mapping;
- source artifact hashes; and
- safe filename generation.

### Database tests

Add transaction-backed tests for:

- migration backfill;
- unique lineage and version identities;
- immutable published rows;
- draft derivation from published versions;
- atomic bundle and included-policy publication;
- source artifact and object mappings;
- exact-match reuse;
- published identity digest conflicts;
- import rollback after any rule failure;
- assignment overlays;
- system-over-environment precedence;
- same-specificity conflicts;
- deletion restrictions; and
- audit events.

### Parser security tests

Include fixtures for:

- external entity access;
- inline DTD;
- entity expansion;
- excessive depth;
- oversized text;
- excessive attributes;
- duplicate IDs;
- invalid namespaces;
- unsupported extension versions;
- ZIP path traversal;
- ZIP symlinks;
- nested archives;
- excessive expansion ratio; and
- truncated or malformed XML.

### API tests

Test:

- authorization for every mutation endpoint;
- preview without mutation;
- digest mismatch between preview and import;
- structured validation errors;
- exact import plan enforcement;
- XML content type and disposition;
- JSON and TOML export;
- published update rejection;
- assignment validation;
- report-only unresolved controls;
- enforce-mode unresolved-control rejection; and
- no automatic activation or assignment.

### Web-ui tests

Test:

- I/O menu keyboard and outside-click behavior;
- permission-based menu items;
- policy selection mode;
- selected export IDs;
- import preview errors and retry;
- conflict-resolution requirements;
- draft and published states;
- trust confirmation;
- effective-set preview;
- not-checked and error result display;
- loading, empty, error, and success states;
- Deployed as the default scanning tab;
- latest-star display for all rows on a flake's latest commit; and
- no mock fallback during management failures.

### Interoperability fixtures

Add fixtures for:

- one Crystal Forge bundle containing every native policy type;
- one multi-rule custom-check bundle;
- one foreign STIG-like XCCDF benchmark;
- one benchmark with profiles and values;
- one benchmark with unknown standard and extension content;
- one published identity conflict;
- one unbound requirement;
- one opaque unsupported check; and
- one source with CCI, SRG, Rule ID, STIG ID, Group ID, and legacy IDs.

Validate exported fixtures against:

- vendored XCCDF 1.2 schemas;
- `cf-xccdf-1.xsd`;
- an OpenSCAP XCCDF validation command in the Nix test environment; and
- Crystal Forge semantic round-trip tests.

Record a manual compatibility check with a named STIG Viewer 3 version before release. Verify that it displays title, severity, discussion, check, fix, Group ID, Rule ID, STIG ID, CCI, and legacy IDs.

## Verification commands

At minimum run:

```bash
cargo fmt --all --check
nix build .#checks.x86_64-linux.web-ui --no-link
nix flake check --keep-going
```

Also run the focused server, database, XCCDF, and web-ui tests added by this task. Record the exact commands and results in the merge request.

If a long Nix VM check cannot complete locally, report the exact command, elapsed time, and last observed phase. Do not mark it as passed.

## Documentation requirements

Update the CF-XCCDF profile so it matches the implemented schema and behavior.

Add operator documentation for:

- importing foreign STIG/XCCDF content;
- importing a Crystal Forge bundle;
- reviewing and trusting executable policies;
- publishing immutable versions;
- assigning a baseline and creating overlays;
- resolving identity conflicts;
- exporting XCCDF;
- exporting and importing JSON/TOML policies;
- dependency failures for non-global NixOS modules; and
- compatibility levels A through D.

State clearly that v0.1 targets:

- Level A: valid and viewable XCCDF;
- Level B: checklist-usable content; and
- Level C: Crystal Forge executable round trip.

Do not claim Level D generic SCAP execution unless standard executable checks and valid SCAP packaging are added and tested.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria

<!-- AC:BEGIN -->
- [ ] #1 The implementation scope matches the actual delta in design commit `5410121e`; it does not reimplement unrelated shell or scanning features that already exist.
- [ ] #2 CF-XCCDF v0.1 namespace URIs, checking-system URIs, canonicalization version, and extension schema are frozen and documented.
- [ ] #3 XCCDF and Crystal Forge schemas are vendored with provenance and validate in the Nix test environment without network access.
- [ ] #4 Existing policies and compliance bundles are migrated to lineage and version records without changing their current effective behavior.
- [ ] #5 Draft policy and bundle versions are mutable, and published versions are immutable at the database and API layers.
- [ ] #6 Editing a published policy or bundle creates a new draft derived from the published version.
- [ ] #7 Publishing a bundle atomically freezes exact policy versions, ordered membership, metadata, identities, and semantic digests.
- [ ] #8 Bundle assignment enforces the full baseline by default and supports explicit exclusions, additions, value overrides, and report-only mode.
- [ ] #9 One effective-policy resolver is used by evaluation, deployment gates, compliance rollups, assignment previews, and derived exports.
- [ ] #10 System-level policy choices override environment-level choices, environment choices override bundle baseline choices, and unresolved same-specificity version conflicts are reported.
- [ ] #11 The server exports one valid XCCDF 1.2 `Benchmark` per bundle version with one baseline `Profile` and one `Rule` per policy version.
- [ ] #12 XCCDF export preserves standard titles, descriptions, severities, identifiers, references, checks, fixes, applicability, and source IDs.
- [ ] #13 XCCDF export supports all current native policy types and preserves exact multi-rule custom-check semantics.
- [ ] #14 Crystal Forge export and reimport satisfy semantic equivalence for all supported policy and bundle fields.
- [ ] #15 The importer classifies CF-native and foreign XCCDF, reports fidelity, and preserves unsupported content instead of silently dropping or coercing it.
- [ ] #16 Foreign XCCDF rules can be stored as native, manual, external, unbound, or opaque policies without inventing Nix expressions.
- [ ] #17 Imported executable policies and bundles default to draft, disabled, untrusted, and unassigned.
- [ ] #18 A matching published version identity with a different digest produces a blocking conflict and is never silently overwritten.
- [ ] #19 Original imported bytes, hashes, parser metadata, package context, and source-object mappings are preserved.
- [ ] #20 XML and ZIP processing rejects DTDs, external entities, expansion attacks, path traversal, archive bombs, excessive depth, duplicate ambiguous IDs, and configured size-limit violations.
- [ ] #21 Canonical JSON and TOML policy import/export support every current native policy type and multi-rule custom checks without lossy flattening.
- [ ] #22 Compatibility import accepts the prior simplified custom-check shape and normalizes it to the canonical model.
- [ ] #23 The XCCDF preview API performs no durable mutation, and the import API reparses the file, verifies the preview digest, and commits atomically.
- [ ] #24 The UI uses server preview, validation, reconciliation, import, and export APIs; it does not use browser XML parsing as the authoritative implementation.
- [ ] #25 The Compliance page uses the shared Import / Export menu and provides STIG/XCCDF import, CF bundle import, XCCDF export, and existing evidence export.
- [ ] #26 The foreign STIG import flow shows benchmark/profile metadata, selectable rules, identifiers, check/fix content, implementation state, trust, and fidelity warnings.
- [ ] #27 The CF-native bundle import flow shows identities, digests, exact matches, conflicts, signatures, unsupported content, and dependencies before commit.
- [ ] #28 Published bundle versions are read-only in the UI and provide a clear Create draft action.
- [ ] #29 The bundle assignment editor shows baseline, exclusions, additions, direct policies, conflicts, unresolved controls, and the effective set before save.
- [ ] #30 Compliance rollups and evidence distinguish pass, warn, fail, waiver, not checked, not applicable, and evaluator error.
- [ ] #31 Operational policies in bundles use real deployment, approval, rollout, or scan evidence when available and report not checked when no applicable evidence exists.
- [ ] #32 The Policies page provides import, selected export, all-custom export, and per-policy export with JSON/TOML format choice.
- [ ] #33 Imported policies are persisted through the API, and the Policies page reloads real data after import.
- [ ] #34 Policy management never falls back to mock data on network or server failure.
- [ ] #35 The shared Import / Export menu is keyboard accessible, closes correctly, returns focus, and renders above page content and trays.
- [ ] #36 Scanning opens on the Deployed tab and retains Active & Recent and All configs tabs.
- [ ] #37 The Deployed tab uses complete server data rather than a client-side filter of a paginated queue.
- [ ] #38 Every scan row for the latest known commit of a flake uses the existing gold latest-star treatment, based on complete server data.
- [ ] #39 Database, parser-security, API, round-trip, web-ui, and interoperability fixtures cover the cases listed in this task.
- [ ] #40 Exported fixtures validate against XCCDF 1.2 and `cf-xccdf-1.xsd`, and the tested compatibility versions are documented.
- [ ] #41 `cargo fmt --all --check` passes.
- [ ] #42 `nix build .#checks.x86_64-linux.web-ui --no-link` passes.
- [ ] #43 `nix flake check --keep-going` passes, or any local timeout is reported accurately and CI provides the authoritative result.
- [ ] #44 The CF-XCCDF specification and operator documentation match the implemented behavior and make no unsupported SCAP-execution claim.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
## Current implementation plan

### Scope and delivery structure
This is a cross-cutting, security-sensitive feature with 44 acceptance criteria spanning migrations, versioned domain semantics, XML/ZIP parsing, APIs, UI, Nix packaging, and interoperability validation. Implement it as sequential, independently verifiable phases under this parent task; do not begin UI work before the server interchange and versioning contracts exist.

1. **Freeze interchange contract and test foundation**
   - Confirm the authoritative CF-XCCDF v0.1 profile against the existing policy model.
   - Vendor XCCDF 1.2/extension schemas with provenance, add no-network validation fixtures, and establish parser/archive resource limits.
   - Decide and document Rust XML/ZIP/XSD dependency choices compatible with the Nix build.

2. **Versioned persistence and canonical semantics**
   - Add additive migrations for policy/bundle versions, ordered membership, source artifacts/mappings, trust/audit data, and assignment overlays.
   - Backfill current policy and bundle rows as drafts without changing current effective behavior.
   - Implement typed canonical DTOs and `cf-model-json-1` SHA-256 semantic digests.

3. **Version lifecycle and effective-policy resolver**
   - Implement mutable drafts, immutable published versions, draft derivation, atomic publish, and delete restrictions.
   - Add one version-aware resolver for baseline/exclusions/additions/direct policy precedence and conflict reporting; connect evaluator, deployment gates, compliance, previews, and derived exports.

4. **Interchange services and APIs**
   - Implement bounded server-side XCCDF/XML and ZIP handling, classification, preview/import transactions, source preservation, XCCDF export, and JSON/TOML policy interchange.
   - Add admin-authorized mutation APIs, structured diagnostics, and audit events. Imported content remains draft, disabled, untrusted, and unassigned.

5. **Evidence and assignment APIs**
   - Extend result states and connect authoritative operational evidence where available.
   - Implement version-aware assignment CRUD, enforce/report-only validation, and effective-set previews.

6. **Dioxus UI and exact design delta**
   - Add an accessible reusable Import / Export menu.
   - Replace Compliance and Policies actions with real API-backed import/export flows; remove policy mock fallback for management failures.
   - Add bundle publication/assignment UI and only the requested Scanning Deployed/latest-per-flake delta.

7. **Verification and documentation**
   - Add focused unit, database, API, parser-security, round-trip, schema/OpenSCAP, and web-ui tests.
   - Update the CF-XCCDF profile and operator documentation; run the required formatting, web-ui Nix check, and full flake check.

### Current implementation facts
- Existing policies and bundles are mutable legacy records; no version, digest, trust, source-artifact, XCCDF, assignment-overlay, or interchange APIs exist.
- All native policy types already have typed server configurations, including legacy and ordered multi-rule custom checks.
- The current effective policy query is a de-duplicated environment/system union, so it must be replaced before versioned bundle semantics can be safely enforced.
- The UI's current STIG modal is sample-only, Policies falls back to mock data on API failure, and Scanning is capped to a 50-item queue; none can satisfy this task without server APIs.
- No XML/ZIP/schema-validation crates or XCCDF resources exist. Dependency and packaging choices are a material implementation decision that must be approved before writing code.

### Verification plan
- During each phase: targeted Rust tests and package formatting through `nix develop`.
- Before UI review: `nix build .#checks.x86_64-linux.web-ui --no-link` and browser evidence/screenshots.
- Before task review: focused server/database/parser/API/round-trip tests, vendored-schema/OpenSCAP validation, `cargo fmt --all --check`, and `nix flake check --keep-going`.

### Required approval
Before implementation, approve the phased delivery/subtask breakdown and the dependency-selection phase. These are material architecture and workflow decisions; a single unstructured implementation pass would not be safely reviewable.

Continuation from 8ade2843: implement only CF-native XCCDF reconciliation in two focused commits. First add strict typed CF-native metadata/payload parsing, canonical digest verification, typed reconciliation decisions/conflicts, and planner/native payload tests without changing the completed foreign package pipeline. Second integrate the planner into the existing atomic import transaction with deterministic advisory locks, idempotent reuse/create paths, mappings/audit/HTTP 409/422 handling, and live PostgreSQL tests for exact reimport, conflicts, mixed reconciliation, mapping conflicts, and concurrency. Preserve the existing source-artifact, transaction, excluded-rule, and live foreign-test behavior unless regression tests require changes.

Stabilization gate continuation from c7138212: first add non-ignored live/native concurrency, mixed-reconciliation, identity/digest, bundle, mapping, and idempotent reimport fixtures; verify locked re-read/order and make only minimal reconciliation fixes. Second reproduce and fix GET /api/v1/scanning/deployed with a failing live endpoint test, keeping scanning changes separate. Third run targeted/full checks, update MR description, and report unrun checks explicitly.

Phase 2 assignment mutation slice (isolated branch TASK-412-phase2-atomicity): inspect existing assignment lineage/version schema and audit conventions; introduce immutable assignment-version child records with expected_version_id optimistic concurrency; refactor create/update/deactivate into one transactional mutation service with deterministic advisory lock order (target, bundle lineage, sorted policy lineages, assignment lineage); add typed 409/404/422 error mapping and transactional audit metadata; inject test-only failure points and live rollback/concurrency tests before broader resolver/evaluation integration. Preserve existing assignment route permissions and do not touch unrelated dirty worktree files.

## Refine workflow replacement phase (2026-08-07)
1. Inspect the authoritative ImportStigModal.jsx and PoliciesView.jsx design, then extract the current inline Refine block into focused Dioxus components under packages/web-ui/src/components/compliance/.
2. Replace StigRule's mixed source/local fields with SourceStigRule plus RefinedPolicyDraft and typed assertion/evidence drafts; keep source cards immutable and convert drafts to canonical import DTOs only at submission.
3. Extend preview DTOs for complete source check/fix/identifier/reference/platform data and add exact MapExisting policy-version selection.
4. Remove top-level rule_customizations in favor of action-local customization and update server record construction/validation accordingly.
5. Add focused pure state, serialization, validation, and server importer tests; run targeted Nix checks; commit and push each focused slice to TASK-412-cf-xccdf-interchange.

### Regression correction: pre-evaluation compliance policy loading
- Preserve the complete effective-set digest for compliance/deployment semantics, but add a dedicated canonical evaluation-policy projection/digest for `load_policies_by_configuration_for_eval`.
- Build that projection after assignment resolution, effective-mode filtering, disabled-policy filtering, Nix-evaluated filtering, and CF-agent exclusion; canonicalize/sort rules deterministically and represent an empty set as the digest of `[]`.
- Compare only the evaluation-policy digest for systems sharing a flake configuration; retain conflict detection for actual Nix-evaluation differences.
- Ensure systems with no Nix gates still participate as an empty evaluation set so they cannot inherit another system's gates silently; define the shared-configuration behavior in tests.
- Add focused regression tests for report-only/manual differences, empty-vs-nonempty shared configurations, and actual enforce/Nix expression differences. Keep resolver infrastructure failures retryable rather than deterministic where the existing failure classification permits.
- Verify with targeted server tests and offline/live database checks as available; do not change unrelated TASK-412 interchange/UI scope.

Final assignment UX pass: update `packages/web-ui/src/views/compliance.rs` only to replace visible UUID/JSON assignment inputs with environment/system and policy selectors backed by existing APIs/models, display the exact selected bundle revision with a non-current warning, preview the exact create request before allowing creation, and preserve UUID payload semantics. Verify with formatting and the targeted web-ui build/check available in this worktree.

Remaining correctness gap: extend the existing set-based system effective-policy batch resolver with an optional exact bundle-version filter. The versioned bundle systems query will resolve all applicable systems in one batch, use the requested bundle version's assignments/overlays/direct-policy precedence, and fall back to exact baseline membership only where no matching assignment exists; the legacy no-version query remains unchanged. Add focused pure tests for exact-version policy selection and exclusion/addition behavior, then run offline server cargo check and targeted tests.

### Focused assignment-effective export wiring
- Add a Dioxus API client helper for `GET /compliance/assignments/:assignment_id/xccdf`.
- Add a distinct server-backed effective-assignment XCCDF download action to the existing assignment list cards; retain the baseline bundle-version export action unchanged.
- Do not parse or construct XML in the browser.
- Verify with the repository `web-ui` cargo check and inspect the final diff for focused scope.

Focused test pass in the existing cf-server unit-test modules: exercise exact bundle-version membership inputs and resolver precedence/conflict behavior with deterministic UUIDs, and exercise effective compliance rollups for exclusions, additions, report-only overlays, and value overrides. Avoid new live-DB coverage because the pure helpers already expose the relevant semantics; retain the repository's existing ignored integration-test convention unchanged.

Focused historical assignment fix: add an authenticated read-only bundle-version membership endpoint returning ordered exact policy-version DTOs from compliance_bundle_version_policies; add matching web-ui DTO/client function; load that membership for AssignmentCreatePanel and use exact policy version IDs for exclusions while retaining the existing policy catalog/version IDs for additions; keep the existing server-backed assignment preview unchanged. Verify with web-ui cargo check and inspect the final diff/status.

### Modal design update (`0c92fdf2`)
- Inspect the committed design delta and existing Dioxus refine/policy editor components plus their tests.
- Keep server preview/XCCDF semantics unchanged; add presentation-only STIG discussion extraction and render prefix-classified SRG/CCI metadata from existing preview identifiers.
- Refactor the existing refine and policy-editor bodies into accessible shared-CSS tab panels while retaining all action, validation, persistence, and delete behavior.
- Add focused tests for discussion extraction, tabs/counts/state preservation/navigation reset; run the canonical affected web-ui tests and required formatting/checks.
- Deploy only through the normal dev process and record any manual verification or blocker.

### Deletion failure reproduction and lifecycle completion
- Reproduce bundle and policy deletion against the deployed development service before changing queries; record HTTP response, journal output, SQLSTATE, and FK/trigger name.
- Map actual FK/trigger ownership for bundle/policy lineages and version children from migrations/schema; distinguish disposable owned children from external/history blockers.
- Implement only the transactionally required owned-child deletion order; retain external references and immutable history as typed conflicts.
- Add PostgreSQL-backed query/API regression tests and run the deployed runtime matrix before reporting this slice complete.

Parity pass for Refine Policy Modal against ImportStigModal.jsx. Root causes: (1) cf-modal-tabs and cf-modal-tab-panel are independent bordered siblings; design wraps in one refine-tab-card. (2) refine-basics margin-bottom 4px vs design 14px. (3) Header gap 16px/icon 15px/progress margin-top 10px vs design 10px/14px/8px. (4) refine-source-card__body gap 10px vs design 14px. (5) refine-source-identifiers no margin-bottom vs design 6px. (6) refine-source-title 12.5px vs design 14px/600/1.4. (7) refine-source-copy 12px/1.5 vs design 12.5px/1.6. (8) refine-source-check pre needs 11.5px/1.6. (9) Enforcement tab and summary use raw .len() not filled assertion count. (10) No scroll containment for long content. Changes: refine_policy.rs: wrap tabs in div.refine-tab-card, group identifiers+title in div.refine-source-heading, fix icon size to 14, use filled assertion count. app.css: add .refine-tab-card rule, remove border/radius from .cf-modal-tabs and .cf-modal-tab-panel, update spacing/typography to design values, add overflow-y:auto scroll containment.

### 2026-08-09 — Policies UI pass
1. Consolidate UI-only inferred policy categories to Deployment, Pipeline gates, Rollout control, and Security & hardening; reuse the shared model in list/filter/card/drawer and editor category selection without persisting category.
2. Port only policy-specific category/group CSS, then update Policies markup to use the corresponding semantic classes.
3. Enrich cards from existing version, mapping, rule, state, and system-count data; add a revision footer that opens the detail drawer directly on Revisions without changing assignment or publication state.
4. Rebuild the drawer presentation around identity/actions, five stable stat cells, Details/Revisions navigation, revision rows, styled rule cards, exact revision mappings, conditional usage text, and collapsed raw definition. Do not fabricate owner, rationale, evidence, or named systems; render `—`/conditional omissions as appropriate.
5. Add/adjust focused web-ui coverage if existing harness data permits, then run web-ui tests, cargo fmt, git diff --check, and the Nix web-ui check. Commit and push this UI pass before addressing the separately requested P0 lifecycle/digest fixes.

### P0 remediation: canonical policy JSON/TOML interchange
1. Define one typed policy-interchange representation that carries every persisted semantic field required by the canonical policy-version model: compliance metadata, dependencies, opaque XML, and default-enabled state in addition to identity/version fields.
2. Build its semantic digest exclusively through `PolicyVersionCanonical`; use that same construction for JSON/TOML export, preview/import validation, and imported-row persistence.
3. Add regression tests proving export-shaped documents verify with the authoritative digest and that changes to the previously omitted semantic fields are both retained and digest-sensitive.
4. Run focused server formatting/tests/checks; explicitly skip `nix build .#checks.x86_64-linux.web-ui --no-link` per user instruction and leave it to CI.
5. Commit/push this isolated digest/interchange fix before starting the separate draft/publication-lifecycle P0 remediation.

### P0 remediation: transactional draft derivation
1. Replace the handler-local policy draft INSERT/pointer update with the query-layer derivation service so one transactional path copies all semantic fields.
2. Extend that service only as needed to preserve an explicitly requested draft version, recompute the `PolicyVersionCanonical` digest after the new draft pointer is set, and fail/roll back on any digest error.
3. Complete the bundle derivation path by recomputing its membership-aware bundle digest and each copied assignment-overlay digest before return/commit.
4. Add focused lifecycle tests for copied semantic fields and absence of `pending` digests, then commit/push separately.

### Review correction for `499a905d`
1. Split explicit policy-draft derivation from `ensure_policy_draft`: explicit POST validates a published source and either creates the requested draft or returns a typed conflict for an existing mutable draft; mutation paths retain reuse semantics.
2. Replace the bundle POST handler's standalone SQL with an explicit transactional derivation service accepting actor and requested version.
3. Do not clone assignment lineages when deriving a bundle draft. Active lineages remain bound to their currently accepted bundle version; inactive lineages remain inactive.
4. Refactor assignment snapshot construction so a canonical overlay digest is calculated from copied overlay content before inserting any immutable assignment-version row; the version and current lineage projection receive the same final digest.
5. Add/run the requested live PostgreSQL regression tests, including no-published policy behavior, draft conflict/version behavior, canonical field preservation, bundle atomicity, assignment invariants, and failure rollback. Skip the unrelated web-ui Nix check.

### TASK-412 Policies domain-model delta (MR !313)
- Limit implementation to `packages/web-ui/src/views/policies.rs` and its existing unit tests; leave shared components, CSS, API DTOs, and excluded untracked assets untouched.
- Replace category stat cards with accessible Platform/Security tabs. Derive domain strictly from `category`, preserving platform category filtering and security grouping selection across tab changes.
- Pivot security lineages with pure helpers for predefined metadata groupings, historical-revision metadata search, and remediation classification from the version config; retain fallback groups so controls are never omitted.
- Reuse `PolicyCard` callbacks and selection state for every resulting group. Verify `cargo fmt --all` and `SQLX_OFFLINE=true cargo check --manifest-path packages/web-ui/Cargo.toml`; do not run the web-ui Nix check or commit/push.

### Custom compliance grouping schemes
- Add server-wide durable grouping schemes because the server has no organization settings scope.
- Add additive migration, typed DTOs, query functions, authenticated GET and admin-only POST/PUT/DELETE routes at `/api/v1/compliance/grouping-schemes`.
- Normalize and validate bounded queries, names, IDs, and policy UUID lists without resolving policy lineages; exclusion IDs take precedence over pinned IDs.
- Cover normalization and route authorization behavior with focused server tests; run `cargo fmt --all --check`, `SQLX_OFFLINE=true cargo check -p cf-server`, and the focused tests only.

### Phase 3: policy grouping schemes
- Add web-ui DTOs and CSRF-aware CRUD client methods for the existing authenticated/admin grouping-scheme endpoints.
- Load schemes alongside policies; add them to the Security grouping selector without impacting built-in group behavior on errors or empty results.
- Provide an admin-only modal using established modal classes to create, update/select/delete schemes and edit group query, descriptions, lineage-ID pins, and exclusions.
- Apply custom matching after existing filters: case-insensitive searchable metadata substring, pins first, exclusions win, first configured group owns a control, and remaining security controls appear in Ungrouped.
- Add pure matching tests and run cargo fmt, offline web-ui cargo check, and focused tests. Do not run the web-ui Nix build per user instruction.

Phase 4 policy classification UI/data wiring: extend the web-ui CRUD request DTOs, seed the unified PolicyEditorModal from the selected current policy, and persist domain/category/framework classification, severity, framework-specific metadata, and rationale. Use the loaded policy library for custom framework suggestions; retain unsupported rule/evidence safeguards. Render selected-revision classification in PolicyDrawer and actual security severity on cards. Verify with focused pure tests, cargo fmt --all --check, and SQLX_OFFLINE cargo check -p web-ui; do not run the web-ui Nix build.

Phase 7 bundle framework UX: add a pure catalog in `packages/web-ui/src/views/compliance.rs` that recognizes the four standard names plus legacy CMMC as an alias, derives distinct non-standard framework values from loaded bundle and policy metadata without browser storage, and renders it in both bundle modals. Replace the selector with Standard and conditional Custom optgroups plus an inline define-new input that accepts Enter/Add and cancels with Escape/Cancel. Preserve the selected persisted string in create/update payloads. Add catalog unit tests and verify with cargo fmt, SQLX_OFFLINE cargo check -p web-ui, and focused tests; do not run the web-ui Nix check.

### Phase 9/10 classification integration
- Preserve full `compliance_metadata` values in the existing CF-native JSON/TOML and CF-XCCDF extension paths; add focused round-trip coverage including unknown keys.
- Add a conservative foreign DISA/STIG classification projection only when benchmark source metadata identifies DISA/STIG, retaining parsed severity and existing SRG/CCI mappings without inferring unrelated classification fields.
- Add explicit semantic-digest coverage for every classification key, then run Rust formatting, package checks, and focused server tests. Do not run the web-ui Nix check.

### Evidence resolver increment (2026-08-10)
- Replace synthesized health/CVE evidence in the system evidence endpoint with evidence resolved against the system's selected deployment target (`systems.desired_target` -> `derivations`).
- Use persisted `derivations.policy_results.assigned` keyed by policy lineage for Nix-evaluated policies; missing or malformed values remain `not_checked` / `error`, never inferred from heartbeat health.
- Use only the latest completed CVE scan for that derivation for `require_cve_check`; no completed scan remains `not_checked`.
- Keep time-window, approvals, canary, manual, external, unbound, and opaque policies `not_checked` until their deployment-decision/manual-evidence context is durably available; do not manufacture a live read-time result.
- Make evidence endpoint and affected rollups share resolved statuses, add unit tests for JSON result decoding/CVE mapping, and align missing web UI rollup fields.
- Verify with focused server and web-ui Rust tests plus formatting; skip the user-directed web-ui Nix check.

### Evidence parity and per-bundle attribution correction (2026-08-10)
- Introduce one explicit effective-policy evidence materialization helper carrying lineage/version identity, exact-version metadata, effective config, mode, and centralized enabled state; use it in both drawer and effective rollups.
- Replace combined-set-to-primary-bundle attribution with resolver-backed per-bundle effective sets. Keep direct policies explicitly outside bundle rows.
- Make no-version bundle-system reads select the same authoritative current version/assignment-aware semantics as versioned reads, including post-delete selection paths.
- Add transaction-backed PostgreSQL coverage for multi-assignment overlays, precedence, report-only attribution, and drawer/matrix/system-rollup parity from persisted deployed Nix/CVE evidence.
- Run requested fmt/check/live compliance tests using only the repository PostgreSQL instance; do not run the web-ui Nix check unless web-ui sources change.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Deletion lifecycle pass: inspected deployed service name crystal-forge-server.service and recent logs; no delete request failures were present in the queried two-hour window. Live API reproduction was unavailable because the deployed OIDC endpoint/listening port is not accessible from this session. Implemented transactional bundle deletion outcomes for missing, disposable, immutable-history, and active-assignment cases with typed 409 ApiError responses; kept DB trigger defense-in-depth. Implemented transactional policy lineage deletion checks covering accepted/deprecated versions, legacy bundle/policy references, exact version bundle membership, assignment additions/exclusions/overrides, and legacy environment/system assignment. Added visible policy and bundle delete errors, busy-state duplicate-delete prevention, lifecycle-aware confirmation text, and bundle delete hiding/reason text for immutable history/known assignments. SQLX_OFFLINE=true cargo check and web-ui cargo check pass; DB-backed tests/runtime verification remain pending due unavailable local database/OIDC endpoint.

Implemented a local, uncommitted first pass of the `0c92fdf2` modal structure: shared CSS tabs in STIG refinement and policy editor; source tab uses existing preview metadata, prefix-filtered/sorted SRG/CCI identifiers, clean `VulnDiscussion` presentation, official full check/fix; navigation resets refinement to Source; custom Nix editors use `code-editor`; lifecycle/action/payload code was not altered. Verification passed: `cargo check --manifest-path packages/web-ui/Cargo.toml`; full web UI Rust tests (144 passed, 1 ignored); both applicable fmt checks; `git diff --check`. Browser component/Playwright coverage and user-authorized dev deployment/manual testing remain required before review.

2026-08-09: Ownership transferred by the user to @gpt-5.6-terra for a narrowly scoped Policies view and policy-detail drawer visual/information-architecture parity pass. Backend correctness findings quoted in the request are recorded for review but are not part of this pass unless explicitly expanded.

2026-08-09 research: existing list DTO exposes policy `updated_at` and revisions' `created_at`, but not owner, persisted ATO evidence, or system membership details. This UI pass will show real modified data where mapped; it will not fabricate unavailable data. Named system membership and evidence require later API/persistence work.

2026-08-09: Policies presentation pass is committed and pushed as `1ced5e95`; no current tracked changes in TASK-412 worktree. Beginning the first backend P0 from review: policy JSON/TOML export/import currently omits `compliance_metadata`, `dependencies`, `opaque_xml`, and `enabled_by_default`, while import computes a reduced non-authoritative digest.

2026-08-09: Pushed `e1e36c75 fix(compliance): preserve policy interchange semantics`. JSON/TOML policy export and exact-version export now include compliance metadata, dependencies, opaque XML, and enabled-by-default. Import persists those fields and computes/verifies digest only through `PolicyVersionCanonical`. Focused parser tests and `cargo check -p cf-server` pass; web-ui Nix check intentionally not run per user direction.

2026-08-09: Pushed `499a905d fix(compliance): finalize derived version digests`. Policy draft endpoint now delegates to the transactional query service, retains optional caller version strings, copies all semantic fields, and finalizes digest before commit. Bundle draft derivation now finalizes membership-aware bundle digests and copied assignment-overlay digests in the same transaction. Focused compliance handler tests: 16 passed, 70 DB-dependent tests ignored; formatting and `cargo check -p cf-server` pass. Web-ui Nix check intentionally not run per user direction.

2026-08-09 review correction: `499a905d` must be fixed forward. The explicit policy draft endpoint incorrectly reused an existing mutable draft through `ensure_policy_draft`; the bundle draft endpoint still has standalone non-transactional SQL; and the bundle helper currently duplicates active assignment lineages and creates immutable assignment snapshots with `pending` digests.

2026-08-09 safety constraint: all live PostgreSQL regression tests for this task must use the repository's isolated process-compose database (`nix develop` → `db-only up` / repository scripts). Do not connect to or mutate the system PostgreSQL on port 5432. Verify the repository-provided connection settings before running DB tests.

2026-08-09: Pushed `45e1a797 fix(compliance): correct explicit draft derivation`. Explicit policy/bundle draft endpoints now use transactional derivation services, accept requested versions, return typed 422/409/500 lifecycle outcomes, and do not silently reuse mutable drafts. Bundle draft derivation copies no assignment lineages, so active assignments remain on accepted versions and inactive assignments are not resurrected. No immutable assignment snapshot is created with `pending` by bundle derivation. Isolated process-compose PostgreSQL was verified at 127.0.0.1:3042 (not system port 5432). Live tests passed: policy no-published 422, policy derivation, bundle derivation, and 68/70 ignored compliance tests. Two full-suite failures were unrelated fixture/database contamination: duplicate generated policy UUID in `all_consumers_agree_on_effective_set_digest_and_specificity`, and a test system insert missing required public_key in `assignment_list_contract_and_deactivation_safety`. Web-ui Nix check not run.

Implemented six P1 review fixes in the TASK-412 worktree: trust gates for policy/bundle publication and effective assignment paths, selected-only baseline and publication handling, persisted assignment addition order, order-independent override canonicalization/application, and inactive assignment effective-resolution rejection. Verification: cargo fmt --all --check; nix develop --command cargo check -p cf-server; nix develop --command cargo test -p cf-server compliance:: --lib (333 passed, 72 ignored).

Implemented the requested Policies domain-model delta in `packages/web-ui/src/views/policies.rs`: accessible Platform/Security tabs, lineage counts, platform-only category controls, retained security grouping selection, predefined metadata pivots and fallbacks, historical revision metadata search, and pure remediation classification with a focused unit test. Verification: `cargo fmt --all --check` from `packages/web-ui` passed; `SQLX_OFFLINE=true cargo check --manifest-path packages/web-ui/Cargo.toml` passed (existing warnings); `SQLX_OFFLINE=true cargo test --manifest-path packages/web-ui/Cargo.toml remediation_status_uses_only_rule_mechanisms` passed (1 test). The requested web-ui Nix check was not run. No commit or push was made.

Implemented server-wide custom compliance grouping schemes in the TASK-412 worktree. Added migration `0209_compliance_grouping_schemes.sql`, typed DTOs, authenticated GET plus admin-only POST/PUT/DELETE routes, normalization (trim, nonempty unique group IDs/case-insensitive names, 4 KiB query bound, UUID-list dedupe, exclusions overriding pins), and focused tests. No policy lineage resolution or digest work was added. Verification passed: `cargo fmt --all --check`; `SQLX_OFFLINE=true cargo check -p cf-server`; `cargo test -p cf-server --lib grouping_scheme_normalization` (2 passed). The requested web-ui Nix check was not run.

Phase 3 grouping-scheme UI implemented in the TASK-412 worktree. Added web-ui DTOs and authenticated/admin CRUD client methods; Policies asynchronously loads schemes without blocking built-in grouping; admin modal supports create/select/update/delete and group query/descriptions/lineage-ID pins/exclusions. Custom groups preserve filters, assign first matching security control only, and render unmatched controls under Ungrouped; exclusion takes precedence over pins. Verification passed: `cargo fmt --all --check --manifest-path packages/default/Cargo.toml`; `SQLX_OFFLINE=true cargo check --manifest-path packages/web-ui/Cargo.toml`; `SQLX_OFFLINE=true cargo test --manifest-path packages/web-ui/Cargo.toml custom_group` (2 passed). The requested web-ui Nix build was intentionally not run. Preserved untracked `U_Anduril_NixOS_V1R1_STIG.zip` and `packages/web-ui/assets/tailwind.css`.

Phase 4 policy editor classification wiring implemented in the dedicated worktree. Added category/framework/severity/control-family/CMMC/CIS/rationale fields to web-ui create/update DTOs; unified Details now has Platform/Security controls, framework-specific inputs, loaded-record custom framework suggestions, and persisted save mapping. Drawer renders the selected revision's classification/rationale; cards render only actual security severity. Verification: `SQLX_OFFLINE=true nix develop --command cargo check` in `packages/web-ui` passed; `nix develop --command cargo test custom_frameworks_excludes_standard_and_empty_values` passed (1 test). `cargo fmt --all --check` passed for `packages/default`, but the `packages/web-ui` workspace check fails on pre-existing formatting in `api/client.rs`, `components/policy/grouping_schemes_modal.rs`, `components/policy/mod.rs`, and two existing blocks in `views/policies.rs`; those unrelated diffs were intentionally not retained. Web-ui Nix build was not run per user instruction.

Phase 7 bundle framework UX implemented in `packages/web-ui/src/views/compliance.rs`: New and Edit bundle modals now share a Standard/Custom framework field sourced from loaded bundle and policy metadata, with modal-only new-framework entry, Enter/Add acceptance, Escape/Cancel discard, and legacy CMMC preservation. Added pure catalog/legacy alias tests. Verification: `nix develop -c env SQLX_OFFLINE=true cargo check --manifest-path packages/web-ui/Cargo.toml` passed (existing warnings); `nix develop -c env SQLX_OFFLINE=true cargo test --manifest-path packages/web-ui/Cargo.toml views::compliance::tests` passed (3 tests); `rustfmt --edition 2024 --check packages/web-ui/src/views/compliance.rs` passed; `git diff --check` passed. `cargo fmt --all --check` was run via the web-ui manifest but is blocked by unrelated existing formatting differences in `src/api/client.rs`, `src/components/policy/grouping_schemes_modal.rs`, `src/components/policy/mod.rs`, and `src/views/policies.rs`; this change is formatted. The web-ui Nix check was intentionally not run per user instruction.

Phase 9/10 classification integration completed locally. Foreign XCCDF imports now capture Dublin Core publisher and project `category=security` / `framework=DISA STIG` only for foreign benchmarks whose publisher is DISA; severity and rationale are copied only from their standard source elements, and SRG/CCI behavior is retained. CF-native XCCDF plus JSON/TOML retain full `compliance_metadata`, including unknown keys. Added focused digest and round-trip tests. Verified with `cargo fmt --all --check`, `cargo check -p cf-server`, focused `cargo test -p cf-server ... --lib` filters, and `git diff --check`; repository pre-existing warnings remain. Did not run the web-ui Nix check. Preserved untracked `U_Anduril_NixOS_V1R1_STIG.zip` and `packages/web-ui/assets/tailwind.css`.

2026-08-10: Replaced the evidence endpoint's heartbeat/placeholder-CVE synthesis for Nix and `require_cve_check` policies. It now resolves only against the latest deployed system state and its matching derivation; Nix uses persisted per-lineage `policy_results`, CVE uses the newest completed derivation scan, and missing/malformed evidence maps to `not_checked`/`error`. Added coverage for lineage-keyed result decoding and aligned the web-ui rollup DTO fields. Focused server tests and web-ui cargo check passed. Full workspace formatting remains blocked by pre-existing formatting differences in previously changed web-ui grouping files; modified files pass direct rustfmt check.

2026-08-10: Extended authoritative evidence use to bundle/system matrices and effective-assignment rollups. Assignment overlays retain their effective config and report-only count while outcome status comes from the same deployed Nix/CVE evidence resolver. The system effective-policy endpoint now returns an internal error rather than a synthetic score if evidence resolution fails. Focused server tests and web-ui cargo check passed.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
created: 2026-08-08 15:45
---
Pushed commit 74150922 to TASK-412-cf-xccdf-interchange. The branch is not yet review-ready against the full final-pass checklist; major correctness and verification gaps remain as documented in Implementation Notes.
---
<!-- COMMENTS:END -->

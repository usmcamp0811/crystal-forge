---
id: TASK-412
title: Implement CF-XCCDF bundle and policy interchange and design updates
status: In Progress
assignee:
  - gpt-5.6-terra
created_date: '2026-08-01 01:04'
updated_date: '2026-08-01 01:52'
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
ordinal: 400000
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
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Dedicated worktree created at `/home/mcamp/code/crystal-forge/TASK-412-cf-xccdf-interchange` on branch `TASK-412-cf-xccdf-interchange`, based on `dev` at `2fdbfa839544628aad4bc802b71d7988cdedc60a`. Initial task research and scope planning are underway.

User approved the recorded seven-phase delivery plan on 2026-07-31. Proceeding with phase 1: freeze the interchange contract and establish vendored-schema/parser test foundations.

Phase 1 foundation added: `cf-server::compliance::interchange` freezes the XCCDF and Crystal Forge namespace/check-system identifiers, canonicalization/digest versions, and bounded XML/ZIP/parser resource limits with unit tests. Added the initial typed `cf-xccdf-1.xsd` extension schema and provenance record. Verification: `SQLX_OFFLINE=true cargo test --manifest-path packages/default/crates/cf-server/Cargo.toml --lib compliance::interchange` passed (2 tests); `xmllint --noout schemas/cf-xccdf-1/cf-xccdf-1.xsd` passed; `git diff --check` passed. A broad `cargo test` without `SQLX_OFFLINE=true` cannot compile because it attempts database connections; broad `rustfmt --check` also reports pre-existing formatting differences in unrelated server files, which were not retained. Remaining phase-1 work: vendor the complete NIST XCCDF 1.2 schema dependency set and add the no-network Nix validation check.

Completed remaining phase-1 schema foundation. Added `xccdf-1-2-schemas`, a pinned Nix package that stages the OpenSCAP-provided XCCDF 1.2.1, CPE language/naming, and XML namespace schemas into a self-contained output. Added the `xccdf-schema` Nix check, which validates a minimal XCCDF Benchmark and CF native custom-check fixture with `xmllint`; `nix build .#checks.x86_64-linux.xccdf-schema --no-link` passed. The check uses only vendored Nix-store schema inputs and performs no runtime schema retrieval. Proceeding to phase 2 migrations/canonical model work.
<!-- SECTION:NOTES:END -->

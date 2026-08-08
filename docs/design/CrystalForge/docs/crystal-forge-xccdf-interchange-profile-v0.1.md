# Crystal Forge XCCDF Interchange Profile

**Abbreviation:** CF-XCCDF  
**Version:** 0.1 Draft  
**Date:** 2026-07-31  
**Status:** Design draft for implementation review

> **Implementation status for this branch:** The server-side CF-XCCDF path is
> implemented for the supported policy and bundle version model. Bundle and
> policy lineages are distinct from exact version identities; the catalog may
> select a current published revision, but export and assignment APIs require
> an explicit version ID. Draft revisions are mutable and accepted revisions
> are immutable. Assignment overlays are resolved server-side and can be
> exported as an effective derived benchmark. Preview/import parsing is
> server-side and digest-checked. The compatibility claims in this document
> are limited to the tested behavior described in Section 22; design-draft
> features not listed there are not implementation claims.

## 1. Purpose

This specification defines how Crystal Forge imports and exports compliance bundles and policies using XCCDF 1.2 XML.

The format has two simultaneous purposes:

1. A Crystal Forge export must be importable by another Crystal Forge installation without losing the bundle's policy definitions or intended behavior.
2. The same document must remain a conforming, useful XCCDF benchmark when opened by standards-based viewers and checklist tools that do not understand Crystal Forge extensions.

The public interchange format is XCCDF. Crystal Forge does not define a competing top-level benchmark format. Crystal Forge adds a small XML extension for executable policy semantics that XCCDF does not natively describe.

## 2. Normative language

The terms **MUST**, **MUST NOT**, **SHOULD**, **SHOULD NOT**, and **MAY** indicate requirement levels in this specification.

## 3. Design principles

### 3.1 XCCDF remains authoritative for standard benchmark content

Information that has a standard XCCDF representation MUST be written to the standard XCCDF field. Crystal Forge metadata MUST NOT replace titles, descriptions, severities, identifiers, references, profiles, checks, fixes, values, platforms, or result fields.

### 3.2 Crystal Forge policies remain the executable unit

A Crystal Forge policy is the primary unit of enforcement and evidence collection. One exported Crystal Forge policy version is represented by one XCCDF `Rule` unless the export is preserving an imported foreign benchmark without normalization.

A multi-expression Crystal Forge policy remains one XCCDF `Rule`. Its component expressions are represented inside the Crystal Forge check content and collectively produce the policy result.

### 3.3 A bundle is a baseline, not a sealed assignment

A compliance bundle defines a default set of policies. Assigning the bundle to an environment or system MUST enforce all selected bundle policies by default.

An assignment MAY:

- exclude policies from the baseline;
- add policies that are not in the baseline;
- override permitted parameter values;
- add system-specific or environment-specific policies; and
- retain local operational policies alongside framework-oriented policies.

The published bundle version remains unchanged. Local additions and exclusions are assignment overlays or derived bundles.

### 3.4 Publication immutability is separate from tailoring

Draft bundle and policy versions are mutable. Published versions are immutable.

Crystal Forge MUST NOT use the XCCDF `prohibitChanges` property as the primary representation of publication immutability. That property can prevent the tailoring that Crystal Forge intentionally permits. Immutability is enforced through stable version identities, content digests, signatures, and Crystal Forge storage rules.

### 3.5 Standard consumers receive useful content without Crystal Forge

A consumer that does not support the Crystal Forge checking system must still be able to:

- load the benchmark as XCCDF;
- browse groups and rules;
- read titles, discussions, checks, fixes, and rationale;
- see severity and framework identifiers;
- select a profile;
- create or complete a manual checklist where supported; and
- process standard XCCDF result values.

A generic scanner is not expected to execute a Crystal Forge Nix or deployment policy. It may report such a rule as `notchecked` because the checking system is unsupported.

## 4. Scope

### 4.1 In scope for version 0.1

- XCCDF 1.2 benchmark import and export
- Crystal Forge-native policy round trips
- Generic STIG/XCCDF import
- Standard XCCDF profiles and values
- Bundle publication and version identity
- Bundle assignment overlays
- Imported source preservation
- XCCDF `TestResult` mapping is specified, but export is not implemented in this branch
- Current Crystal Forge policy types
- NixOS configuration checks using the Crystal Forge `cfg` evaluation context
- Operational policies, including approval, time-window, rollout, and vulnerability controls

### 4.2 Out of scope for version 0.1

- Claiming that Crystal Forge-authored content is an official DISA STIG
- Full SCAP source data stream production
- Automatic conversion of arbitrary checks to OVAL
- Generic scanner execution of Crystal Forge policy checks
- Automatic installation of flake inputs or NixOS modules referenced by imported policies
- Byte-for-byte regeneration of modified foreign XCCDF documents
- A portable representation of local Crystal Forge environment UUIDs, system UUIDs, users, roles, or approval records

## 5. Terminology

**Bundle lineage**  
A stable identity for a logical compliance bundle across versions.

**Bundle version**  
A versioned snapshot of a bundle and its baseline policy membership.

**Policy lineage**  
A stable identity for a logical policy across versions.

**Policy version**  
A versioned policy definition, including its executable configuration.

**Baseline profile**  
The XCCDF profile that selects every policy included in the published bundle baseline.

**Assignment overlay**  
Local exclusions, additions, and value overrides applied when a bundle is assigned.

**Derived benchmark**  
A new benchmark snapshot that combines a source bundle with additions or other changes that cannot be represented by XCCDF tailoring alone.

**CF-native document**  
An XCCDF document containing a recognized Crystal Forge extension and Crystal Forge policy checking system.

**Foreign document**  
An XCCDF document that does not contain a recognized Crystal Forge policy representation.

**Semantic round trip**  
An export, import, and re-export cycle that preserves the meaning and executable behavior of the Crystal Forge objects, without requiring identical XML whitespace, namespace prefixes, or element formatting.

## 6. Conformance classes

An implementation MAY claim one or more conformance classes.

### 6.1 CF-XCCDF Producer

A producer:

- emits XCCDF 1.2 XML;
- follows the mappings in this specification;
- emits valid Crystal Forge extension content;
- emits stable identities and content digests; and
- does not require a sidecar file for Crystal Forge reimport.

### 6.2 CF-XCCDF Consumer

A consumer:

- validates and parses XCCDF safely;
- recognizes supported Crystal Forge extension versions;
- reconstructs supported policy types;
- preserves unsupported standard and extension content; and
- does not silently activate imported executable content.

### 6.3 CF-XCCDF Round-Trip Consumer

A round-trip consumer additionally satisfies the semantic preservation requirements in Section 17.

### 6.4 Viewer-compatible Producer

A viewer-compatible producer demonstrates that its exported benchmark can be loaded by the selected compatibility test tools and that the required human-readable fields are visible.

### 6.5 SCAP-executable Producer

This optional, future class requires standard executable check content such as OVAL or OCIL and any required SCAP packaging. A CF-XCCDF document containing only Crystal Forge checks MUST NOT claim this class.

## 7. XML namespaces and checking systems

Version 0.1 proposes the following identifiers:

```xml
xmlns:xccdf="http://checklists.nist.gov/xccdf/1.2"
xmlns:cf="urn:crystal-forge:xccdf:1"
```

Crystal Forge policy checks use:

```text
urn:crystal-forge:check-system:policy:1
```

Crystal Forge Nix remediation content may use:

```text
urn:crystal-forge:fix-system:nix:1
```

These identifiers MUST be frozen before the first public release. A future incompatible extension MUST use a new namespace or check-system version.

## 8. Portable artifact

The required portable artifact is one standalone XCCDF 1.2 XML document.

All information required for a Crystal Forge semantic round trip MUST be embedded in that XML document. `check-content-ref` MAY be used as an alternative location, but a self-contained `check-content` fallback MUST be present when external content is required for exact reimport.

A ZIP package MAY contain:

- the benchmark XML;
- schemas;
- human documentation;
- evidence attachments;
- standard check content;
- detached signatures; and
- a package manifest.

The standalone benchmark XML remains the normative bundle definition.

## 9. Core XCCDF mapping

| Crystal Forge object | XCCDF representation |
|---|---|
| Published or exported bundle version | `Benchmark` |
| Bundle baseline | `Profile` selecting the bundle rules |
| Policy version | `Rule` |
| Policy category or logical section | `Group` |
| Tailorable policy parameter | `Value` |
| Nix or Crystal Forge policy evaluator | `check` with Crystal Forge checking-system URI |
| Human verification procedure | Rule description and Crystal Forge check procedure |
| Human remediation guidance | `fixtext` |
| Executable Nix remediation | `fix` with Crystal Forge Nix fix-system URI |
| CCI, SRG, STIG, CIS, NIST, or local identifiers | `ident` and `reference` |
| Applicable platform | `platform` and optional Crystal Forge dependency metadata |
| Assessment run | `TestResult` |
| Policy result | `rule-result` |
| Local assignment changes | `Tailoring` or a derived `Benchmark` |

## 10. Benchmark representation

One canonical Crystal Forge bundle version MUST export as one XCCDF `Benchmark`.

The benchmark MUST contain:

- an XCCDF 1.2-compliant `id`;
- `status`;
- `title`;
- `description` when available;
- `version`;
- one Crystal Forge bundle metadata block;
- at least one profile representing the baseline;
- one `Rule` for every exported policy version; and
- any groups, values, references, and platforms required by the content.

### 10.1 Status mapping

| Crystal Forge state | XCCDF status |
|---|---|
| Incomplete local work | `incomplete` |
| Shareable draft | `draft` |
| Release candidate | `interim` |
| Published immutable version | `accepted` |
| Retired version | `deprecated` |

The `status` date SHOULD be emitted.

### 10.2 Bundle metadata

The benchmark MUST contain a metadata block similar to:

```xml
<xccdf:metadata>
  <cf:bundle
      schema-version="1"
      bundle-id="urn:uuid:11111111-1111-1111-1111-111111111111"
      bundle-version-id="urn:uuid:22222222-2222-2222-2222-222222222222"
      publication-state="accepted">
    <cf:framework name="DISA STIG" version="V2R1"/>
    <cf:layer>operating-system</cf:layer>
    <cf:owner>Example Publisher</cf:owner>
    <cf:content-digest algorithm="sha-256">...</cf:content-digest>
  </cf:bundle>
</xccdf:metadata>
```

This metadata provides identity and provenance. It MUST NOT replace corresponding standard XCCDF fields.

### 10.3 Baseline profile

Every benchmark MUST contain one profile designated as the Crystal Forge baseline profile. It MUST explicitly select every policy rule included in the bundle baseline.

The profile metadata MUST identify its purpose:

```xml
<xccdf:metadata>
  <cf:profile-role>baseline</cf:profile-role>
</xccdf:metadata>
```

Rules that are present only as documentation or optional alternatives MUST be explicitly unselected by the baseline profile.

The baseline profile SHOULD remain tailorable. Publishing the benchmark freezes the source artifact; it does not prevent a consumer from creating a local derivative.

## 11. Policy representation

### 11.1 One policy version per rule

A CF-native export MUST represent each included Crystal Forge policy version as one XCCDF `Rule`.

The rule MUST contain:

- an XCCDF rule identifier;
- a title;
- a description or discussion;
- a severity, using `unknown` when no meaningful compliance severity exists;
- a Crystal Forge policy metadata block;
- a Crystal Forge policy check;
- human-readable check information;
- remediation guidance when available; and
- all known external identifiers and references.

`strict` is an enforcement property. It MUST NOT be inferred from XCCDF severity, weight, or role.

### 11.2 Standard human-readable content

The standard portion of the rule SHOULD be useful without Crystal Forge.

For imported STIG content, Crystal Forge MUST preserve, when present:

- Group ID;
- Rule ID;
- STIG or vulnerability ID;
- legacy IDs;
- CCI identifiers;
- SRG identifiers;
- severity;
- rule title;
- discussion;
- check content;
- fix content;
- rationale;
- references;
- version; and
- platform applicability.

For Crystal Forge-authored operational controls, the exporter MUST provide equivalent human descriptions. It MUST NOT invent official-looking DISA `V-` or `SV-` identifiers.

### 11.3 Policy identity metadata

Each CF-native rule MUST contain:

```xml
<xccdf:metadata>
  <cf:policy-identity
      policy-id="urn:uuid:..."
      policy-version-id="urn:uuid:..."
      publication-state="accepted">
    <cf:policy-version>1.2.0</cf:policy-version>
    <cf:content-digest algorithm="sha-256">...</cf:content-digest>
  </cf:policy-identity>
</xccdf:metadata>
```

A local database primary key MAY be recorded as origin information, but MUST NOT be used as the only portable identity.

### 11.4 Crystal Forge policy check

The executable policy is embedded in an XCCDF check:

```xml
<xccdf:check system="urn:crystal-forge:check-system:policy:1">
  <xccdf:check-content>
    <cf:policy schema-version="1">
      <cf:execution phase="nix-evaluation" strict="true"/>
      <cf:implementation>
        <!-- Exactly one typed policy element -->
      </cf:implementation>
    </cf:policy>
  </xccdf:check-content>
</xccdf:check>
```

The `check-content` body MUST contain no XCCDF elements.

The Crystal Forge extension schema MUST permit one typed implementation element and versioned extension points. It MUST NOT use an untyped JSON object as the normative policy representation.

## 12. Current policy-type encodings

The final companion XML schema must define one typed element for each supported policy type. The following structures are normative at the conceptual level for version 0.1.

### 12.1 Require Crystal Forge agent

```xml
<cf:require-crystal-forge-agent/>
```

`strict` is recorded on `cf:execution`.

### 12.2 Require packages

```xml
<cf:require-packages>
  <cf:package>audit</cf:package>
  <cf:package>openssh</cf:package>
</cf:require-packages>
```

Package order is not semantically significant. The importer SHOULD preserve source order for presentation.

### 12.3 Custom Nix check

```xml
<cf:custom-check mode="all" context="nixos-configuration-v1" binding="cfg">
  <cf:rule field-name="firewallEnabled" strict="true">
    <cf:description>The NixOS firewall must be enabled.</cf:description>
    <cf:expression language="nix"><![CDATA[
cfg.config.networking.firewall.enable
    ]]></cf:expression>
  </cf:rule>
</cf:custom-check>
```

Requirements:

- `mode` MUST be `all` or `any`.
- A legacy single-expression Crystal Forge policy MUST normalize to one nested `cf:rule` on export.
- `binding="cfg"` identifies the full `nixosConfigurations.<name>` object supplied by Crystal Forge.
- Expressions using the version 1 context MUST use the `cfg.config.*` lexical contract for NixOS module configuration access.
- Expression text MUST be preserved exactly, except that an importer MAY normalize XML line endings to LF.
- The exporter MUST safely encode expression text. It MUST handle a literal `]]>` sequence instead of emitting invalid CDATA.
- The field name and per-rule strictness MUST be preserved.
- Rule order MUST be preserved because it can affect presentation and evidence ordering, even when it does not change boolean evaluation.

### 12.4 Require CVE check

```xml
<cf:require-cve-check
    max-critical="0"
    require-high-justification="false"
    when-no-scan="block">
  <cf:max-high>10</cf:max-high>
</cf:require-cve-check>
```

The schema MUST distinguish an absent `max-high` value from zero.

### 12.5 Time window

```xml
<cf:time-window
    start-time="09:00"
    end-time="17:00"
    timezone="America/Chicago"
    action="block">
  <cf:description>Approved weekday deployment window</cf:description>
  <cf:day>mon</cf:day>
  <cf:day>tue</cf:day>
  <cf:day>wed</cf:day>
  <cf:day>thu</cf:day>
  <cf:day>fri</cf:day>
</cf:time-window>
```

Times MUST be 24-hour `HH:MM`. Time zones MUST use IANA identifiers. Day values MUST use the normalized lower-case values `mon` through `sun`.

### 12.6 Require approvals

```xml
<cf:require-approvals count="2" role="operator" distinct="true">
  <cf:description>Two distinct operators must approve production deployment.</cf:description>
  <cf:expires-after-hours>24</cf:expires-after-hours>
</cf:require-approvals>
```

An absent expiration value means that approvals do not expire under the policy definition.

### 12.7 Canary rollout

```xml
<cf:canary-rollout
    percentage="25"
    observe-duration-minutes="30"
    selection-strategy="hash-based">
  <cf:description>Deploy in four observed phases.</cf:description>
  <cf:health-check type="systemd" fail-threshold="1"/>
</cf:canary-rollout>
```

### 12.8 CVE threshold

```xml
<cf:cve-threshold
    no-scan-behavior="block"
    allow-justifications="true"
    require-acknowledgment="false">
  <cf:description>Production vulnerability threshold</cf:description>
  <cf:threshold severity="critical" max="0" action="block"/>
  <cf:threshold severity="high" max="5" action="warn"/>
</cf:cve-threshold>
```

Severity keys that Crystal Forge does not recognize MUST be preserved but MUST NOT be activated until supported or explicitly mapped.

### 12.9 Future and unknown policy types

A future policy type MUST use a typed element in a versioned Crystal Forge namespace or an explicitly versioned extension point.

A consumer that does not understand the typed implementation MUST:

- preserve the complete XML subtree;
- mark the policy as unsupported;
- prevent automatic activation; and
- retain the human-readable XCCDF rule.

It MUST NOT silently coerce the policy to another type.

## 13. Policy phases and enforcement

The `cf:execution` element records the Crystal Forge phase at which a policy applies. Version 0.1 defines:

- `nix-evaluation`;
- `post-build`;
- `pre-deployment`;
- `deployment-orchestration`; and
- `continuous-assessment`.

The exact phase MUST be preserved on round trip.

A policy's XCCDF severity describes compliance impact. Its Crystal Forge strictness and action describe operational enforcement. These properties are independent.

Examples:

- A high-severity rule may be non-strict during a transition period.
- A time-window rule may block deployment but have XCCDF severity `unknown`.
- A CVE threshold may warn at one severity and block at another.

## 14. Dependencies and non-global NixOS modules

A policy MAY reference options supplied by modules that are not globally available in NixOS.

The policy remains portable, but the target flake is responsible for providing the referenced module and options.

A policy MAY declare advisory dependencies:

```xml
<cf:dependencies>
  <cf:nix-option path="services.example.enable"/>
  <cf:module-ref uri="github:example/security-module" optional="false"/>
</cf:dependencies>
```

Dependency declarations improve discovery and preflight validation. They are not a replacement for evaluating the policy against the target flake.

Crystal Forge MUST NOT automatically add a flake input or import a module solely because an imported document requests it.

Before an imported bundle can be activated, Crystal Forge SHOULD preflight its Nix-evaluated policies against the target flake. Missing options or modules MUST produce an unresolved-dependency or evaluation-error state, not a false pass.

## 15. Identifiers and framework mappings

### 15.1 XCCDF identifiers

Exported major XCCDF elements MUST use XCCDF 1.2-compatible identifiers following the form:

```text
xccdf_org.crystalforge_benchmark_<stable-key>
xccdf_org.crystalforge_profile_<stable-key>
xccdf_org.crystalforge_group_<stable-key>
xccdf_org.crystalforge_rule_<stable-key>
xccdf_org.crystalforge_value_<stable-key>
```

Crystal Forge MUST NOT depend on the XCCDF identifier alone for exact object reconciliation. Portable UUID or URI identities in the Crystal Forge metadata are authoritative for CF-native round trips.

### 15.2 Imported identifiers

Official imported identifiers MUST be preserved exactly in their standard fields and source metadata. Crystal Forge MUST distinguish:

- the source XCCDF rule identifier;
- STIG or vulnerability identifiers;
- rule version identifiers;
- CCI identifiers;
- SRG identifiers;
- legacy identifiers; and
- Crystal Forge policy identities.

### 15.3 Crystal Forge-authored content

Crystal Forge-authored content MUST use Crystal Forge identities. It MUST NOT mint identifiers that imply DISA publication or approval.

## 16. Bundle assignment and tailoring

### 16.1 Assignment semantics

The effective policy set for an assignment is:

```text
effective = (bundle baseline - excluded policies) + added policies
```

Value overrides and permitted policy-specific configuration overrides are then applied.

The default assignment has no exclusions and no additions. It enforces the full baseline.

### 16.2 Canonical bundle export

A canonical bundle export is selected by an explicit bundle version ID and
contains that exact revision and its baseline profile. A catalog's `current`
selection is not a substitute for the requested version identity. Canonical
export does not contain local environment or system assignment state.

### 16.3 Tailoring export

When an assignment only:

- excludes existing benchmark rules;
- selects existing alternatives; or
- changes values already represented by XCCDF `Value` elements,

Crystal Forge does not currently export an XCCDF `Tailoring` document for this
path. The implemented assignment export endpoint resolves the overlay and
writes a standalone effective benchmark instead.

### 16.4 Added policies

XCCDF tailoring selects and refines content already present in a benchmark. It is not the portable mechanism for adding new `Rule` definitions.

When an assignment adds policies not contained in the source benchmark, Crystal Forge MUST export an effective derived benchmark if a single standalone interoperable document is requested.

The derived benchmark MUST:

- receive its own bundle lineage or derived-version identity;
- contain every effective policy as a rule;
- identify the source bundle and version in `cf:derived-from` metadata;
- record excluded source policies and added policies in provenance metadata; and
- contain a baseline profile that selects the effective policy set.

### 16.5 Recommended product behavior

Crystal Forge SHOULD offer these export choices:

1. **Canonical bundle**: the published reusable baseline.
2. **Assignment tailoring**: a small overlay for a known source benchmark.
3. **Effective benchmark**: one standalone benchmark containing the complete effective policy set.

The effective benchmark is the recommended export for sharing an ad hoc customized baseline with generic tools.

## 17. Round-trip requirements

### 17.1 Semantic equivalence

For a supported CF-native document:

```text
normalize(import(export(bundle))) == normalize(bundle)
```

The comparison is semantic. XML prefix names, indentation, attribute order, and harmless whitespace are not significant.

### 17.2 Required preserved properties

A round trip MUST preserve:

- bundle lineage identity;
- bundle version identity;
- publication state;
- bundle version string;
- bundle title, description, framework, layer, and owner;
- baseline policy membership;
- policy order or explicit display order;
- group structure;
- policy lineage and version identities;
- policy type;
- policy execution phase;
- policy strictness and action semantics;
- every typed configuration value;
- custom-check mode;
- custom-check rule order;
- custom-check expressions;
- field names;
- per-rule strictness;
- standard titles, descriptions, rationale, checks, and fixes;
- severity;
- identifiers and references;
- applicability and dependencies;
- source provenance;
- content digests; and
- unsupported extension content that the importer accepted for preservation.

### 17.3 Content digest

A Crystal Forge content digest MUST be calculated over a documented canonical semantic representation, not raw XML bytes. This avoids changes caused only by formatting or namespace-prefix choices.

The canonicalization algorithm MUST be versioned. The digest metadata MUST identify the algorithm and canonical-model version.

XML signatures, when used, are separate and cover the XML representation according to the applicable signature rules.

### 17.4 Draft conflicts

If a consumer receives the same draft version identity with a different digest, it MAY treat the document as a newer draft revision after showing the difference.

### 17.5 Published conflicts

If a consumer receives the same published policy or bundle version identity with a different digest, it MUST report an identity conflict. It MUST NOT silently overwrite, merge, or activate the conflicting content.

## 18. Import behavior

### 18.1 Parse and validate first

The importer MUST:

1. parse XML with external entities and DTD processing disabled;
2. enforce input, nesting, text, and expansion limits;
3. validate the XCCDF structure;
4. validate recognized Crystal Forge extension content;
5. inspect signatures and digests when present;
6. classify the document as CF-native or foreign; and
7. create an import plan before writing active policy assignments.

### 18.2 CF-native import

For a supported CF-native document, Crystal Forge MUST reconstruct the exact supported policy definitions.

Object reconciliation order:

1. Match by portable policy or bundle version identity.
2. Verify the semantic digest.
3. Reuse an identical immutable version.
4. Create a new version when the lineage matches but the version identity differs.
5. Report a conflict when an immutable identity matches but the digest differs.
6. Never match solely by title or local UUID.

### 18.3 Foreign STIG/XCCDF import

For a foreign benchmark, Crystal Forge MUST import the human and structural content even when it cannot execute the checks.

Each imported XCCDF rule becomes one of:

- a supported external-check policy;
- a supported manual-check policy;
- an unbound requirement policy awaiting an implementation; or
- an opaque unsupported policy record.

The importer MUST NOT invent executable Nix expressions from prose without an explicit user-authoring or assisted-mapping workflow.

A foreign import SHOULD initially create a draft bundle. Unsupported checks remain visible and report `notchecked` until bound to a supported Crystal Forge evaluator.

### 18.4 Source preservation

Crystal Forge MUST preserve the original imported artifact with:

- original bytes;
- original filename;
- media type;
- SHA-256 digest;
- import time;
- parser version;
- detected XCCDF version;
- package context, when imported from ZIP; and
- a mapping from source benchmark, group, rule, profile, and value identities to normalized Crystal Forge objects.

Unknown XML MUST be preserved either as an attached source artifact or as opaque subtrees associated with the normalized object.

### 18.5 Import fidelity states

Each import SHOULD report one of these states:

- **Native exact**: supported Crystal Forge semantics were reconstructed exactly.
- **Normalized complete**: all standard features used by the source were modeled.
- **Preserved opaque**: unsupported features were retained but cannot be edited or executed.
- **Degraded**: information was omitted or transformed in a way that prevents a full semantic round trip.

A degraded import MUST display the lost or unsupported features before publication or export.

## 19. Export behavior

### 19.1 Export modes

The implemented export modes are:

- canonical bundle XCCDF;
- effective assignment XCCDF;
- policy JSON/TOML interchange; and
- source-preserving source-artifact retention for imported content.

XCCDF `Tailoring`, XCCDF `TestResult` export, and a complete byte-preserving
modified foreign-document re-export are not implementation claims for this
branch.

### 19.2 Source-preserving re-export

If a foreign artifact has not been modified, Crystal Forge SHOULD permit re-export of the original bytes.

If normalized content has been modified, Crystal Forge MAY regenerate XCCDF while reinserting preserved opaque content. It MUST report when byte identity or unsupported-source semantics cannot be guaranteed.

### 19.3 Standard validity

Every benchmark export MUST validate against XCCDF 1.2.

A document with Crystal Forge policy checks remains an XCCDF benchmark, but it MUST NOT be advertised as generically automated SCAP content unless standard executable checks and required packaging are also supplied.

### 19.4 Human-check fallback

Every CF-native rule SHOULD contain enough human-readable information for a reviewer to determine what is being checked and how to remediate it.

A producer MAY include a standard alternative check, such as OVAL or OCIL, when equivalent content exists. The Crystal Forge check remains authoritative for exact Crystal Forge reimport unless the document explicitly declares a verified semantic equivalence.

## 20. Assessment result export

Assessment results are separate from the immutable bundle definition.

The result mapping below is a profile specification, not an implementation
claim for this branch. XCCDF `TestResult` export is not currently exposed by the
implemented interchange API.

Crystal Forge MAY export one XCCDF `TestResult` per target assessment. Each policy result maps to one `rule-result`.

Recommended mapping:

| Crystal Forge state | XCCDF result |
|---|---|
| Passed | `pass` |
| Failed | `fail` |
| Evaluator failure | `error` |
| Indeterminate | `unknown` |
| Not applicable | `notapplicable` |
| Unsupported or not evaluated | `notchecked` |
| Excluded by assignment/profile | `notselected` |
| Informational-only result | `informational` |
| Successfully remediated and verified | `fixed` |

A waiver MUST NOT convert the observed result to `pass`. Crystal Forge SHOULD preserve the observed result and export an override, rationale, authority, and expiration information using standard result facilities where possible and Crystal Forge metadata for additional evidence.

Detailed evidence MAY be included in `rule-result` metadata or referenced as package attachments. The standard result remains usable when the Crystal Forge evidence extension is ignored.

## 21. Trust and security

### 21.1 Executable content is untrusted by default

A Nix expression or deployment policy imported from XML is executable content.

Importing a document MUST NOT automatically:

- enable its policies;
- assign its bundle;
- evaluate its expressions;
- add flake inputs;
- import NixOS modules;
- schedule deployments; or
- grant approval authority.

Crystal Forge MUST require an explicit trust or review action before activation, unless a local policy permits automatic trust for a verified publisher signature.

After a bundle is trusted and assigned, the default assignment behavior is to enforce the full baseline.

### 21.2 XML and package hardening

The implementation MUST protect against:

- external entity expansion;
- DTD retrieval;
- entity-expansion attacks;
- excessive element depth;
- oversized text nodes;
- ZIP bombs;
- nested-archive exhaustion;
- path traversal during extraction;
- oversized attachments;
- signature wrapping; and
- ambiguous duplicate identifiers.

### 21.3 Signature and publisher trust

A benchmark MAY carry an XML digital signature. Crystal Forge SHOULD support publisher trust configuration separately from document validity.

A valid signature proves integrity and key possession. It does not, by itself, authorize execution in a local installation.

### 21.4 Evaluation isolation

Imported Nix checks MUST run through the same bounded and isolated evaluation path as locally authored policies. Import must not create an alternate unrestricted execution route.

## 22. Compatibility promise

Crystal Forge MUST describe compatibility using explicit levels.

### Level A: Valid and viewable

- The document validates as XCCDF 1.2.
- Generic XCCDF tools can load standard fields.
- STIG-oriented viewers can display the rules when they support general XCCDF content.

### Level B: Checklist usable

- A reviewer can create or complete a checklist using the standard rule content.
- Results can be imported or exported through supported XCCDF result workflows.

### Level C: Crystal Forge executable

- A Crystal Forge consumer understands the extension version.
- The exact policy can be reconstructed and evaluated after trust and dependency checks.

### Level D: Generic SCAP executable

- Standard automated check content is included.
- Required SCAP packaging is valid.
- A named third-party scanner successfully executes the checks.

This branch has tested server behavior for Levels A through C only. It does not
claim Level D generic SCAP execution. No named third-party viewer or STIG Viewer
release is claimed as compatible unless that exact version has a recorded test;
XCCDF schema validity does not prove that a viewer accepts Crystal Forge
extension content.

## 23. Validation and test suite

A CF-XCCDF implementation MUST include automated fixtures and tests for:

1. XCCDF 1.2 schema validation.
2. Crystal Forge extension-schema validation.
3. Export, import, and semantic comparison for every current policy type.
4. Single-expression custom-check normalization.
5. Multi-rule `all` and `any` policies.
6. Exact preservation of Nix expressions and field names.
7. Draft identity with changed digest.
8. Published identity conflict detection.
9. Unknown Crystal Forge policy preservation.
10. Foreign XCCDF unknown-element preservation.
11. Bundle baseline selection.
12. Assignment exclusions.
13. Assignment additions through derived-benchmark export.
14. Assignment value overrides in effective export.
15. Unsupported check-system handling as `notchecked`.
16. Result and waiver mapping.
17. Malicious XML and archive rejection.
18. Missing custom-module dependency behavior.

The branch's focused tests cover server-side parsing and writing, identity and
digest reconciliation, supported policy JSON/TOML round trips, assignment
resolution, effective-set digest consistency, and canonical/effective XCCDF
export. They do not establish OpenSCAP execution of Crystal Forge checks,
DISA STIG Viewer compatibility, or generic scanner execution. Any such claim
must name the exact tested tool version and fixture.

Tool compatibility MUST be stated against tested versions. Standards conformance alone does not guarantee that every product accepts every valid extension pattern.

## 24. Required Crystal Forge data-model changes

The existing bundle-to-policy relationship can remain, but portable and immutable versioning requires additional concepts.

### 24.1 Bundle storage

Recommended fields or related tables:

- `bundle_lineage_id`;
- `bundle_version_id`;
- `version`;
- `publication_state`;
- `published_at`;
- `semantic_digest`;
- `canonicalization_version`;
- `source_artifact_id`;
- `derived_from_bundle_version_id`; and
- immutable membership rows for published versions.

### 24.2 Policy storage

Recommended fields or related tables:

- `policy_lineage_id`;
- `policy_version_id`;
- `version`;
- `publication_state`;
- `published_at`;
- `semantic_digest`;
- `execution_phase`;
- standard compliance metadata;
- source identities;
- dependency declarations; and
- opaque imported XML.

### 24.3 Assignment storage

An assignment should reference a bundle version and contain:

- scope type and scope identity;
- enforcement mode, defaulting to enforce;
- excluded policy version identities;
- added policy version identities;
- allowed value overrides;
- assignment provenance; and
- effective-set digest.

The bundle version itself is not mutated when an assignment changes.

### 24.4 Imported source storage

A source-artifact table should preserve bytes, hashes, media type, parser information, and object mappings.

## 25. Non-normative complete rule example

```xml
<xccdf:Rule
    id="xccdf_org.crystalforge_rule_firewall-enabled"
    selected="true"
    severity="high">
  <xccdf:status date="2026-07-31">accepted</xccdf:status>
  <xccdf:title>The NixOS firewall must be enabled.</xccdf:title>
  <xccdf:description>
    The evaluated NixOS configuration must enable the host firewall.
  </xccdf:description>
  <xccdf:rationale>
    An enabled host firewall reduces unintended network exposure.
  </xccdf:rationale>
  <xccdf:metadata>
    <cf:policy-identity
        policy-id="urn:uuid:33333333-3333-3333-3333-333333333333"
        policy-version-id="urn:uuid:44444444-4444-4444-4444-444444444444"
        publication-state="accepted">
      <cf:policy-version>1.0.0</cf:policy-version>
      <cf:content-digest
          algorithm="sha-256"
          canonical-model="cf-policy-1">...</cf:content-digest>
    </cf:policy-identity>
  </xccdf:metadata>
  <xccdf:ident system="urn:crystal-forge:policy-id">
    urn:uuid:33333333-3333-3333-3333-333333333333
  </xccdf:ident>
  <xccdf:fixtext>
    Enable networking.firewall.enable in the target NixOS configuration.
  </xccdf:fixtext>
  <xccdf:fix system="urn:crystal-forge:fix-system:nix:1">
    networking.firewall.enable = true;
  </xccdf:fix>
  <xccdf:check system="urn:crystal-forge:check-system:policy:1">
    <xccdf:check-content>
      <cf:policy schema-version="1">
        <cf:execution phase="nix-evaluation" strict="true"/>
        <cf:implementation>
          <cf:custom-check
              mode="all"
              context="nixos-configuration-v1"
              binding="cfg">
            <cf:rule field-name="firewallEnabled" strict="true">
              <cf:description>
                The evaluated firewall option is true.
              </cf:description>
              <cf:expression language="nix">cfg.config.networking.firewall.enable</cf:expression>
            </cf:rule>
          </cf:custom-check>
        </cf:implementation>
        <cf:dependencies>
          <cf:nix-option path="networking.firewall.enable"/>
        </cf:dependencies>
      </cf:policy>
    </xccdf:check-content>
  </xccdf:check>
</xccdf:Rule>
```

The final implementation fixture MUST be validated against the XCCDF and Crystal Forge schemas. This example expresses the intended structure but is not a substitute for the companion schema and conformance tests.

## 26. Decisions captured by this draft

This draft makes the following product decisions explicit:

1. Assigning a bundle enforces all baseline policies by default.
2. Assignments can subtract baseline policies and add ad hoc policies.
3. Policies remain the primary enforcement and evidence mechanism.
4. Operational deployment policies are valid bundle content.
5. Draft versions are mutable and published versions are immutable.
6. Nix policy expressions are portable ecosystem content.
7. Custom module dependencies are allowed but are not installed automatically.
8. A standalone export uses XCCDF 1.2 plus a small Crystal Forge extension.
9. Generic tools receive useful standard content but are not expected to execute Crystal Forge checks.
10. Additive assignment changes require a derived benchmark for a complete standalone generic export.

## 27. Open decisions before version 0.2

1. Choose and freeze the public Crystal Forge XML namespace and check-system URIs.
2. Define the exact XSD for the Crystal Forge extension.
3. Define the canonical semantic digest representation.
4. Decide whether policy and bundle version strings must follow Semantic Versioning.
5. Define a portable publisher identity and trust-store model.
6. Define the minimum manual-check policy type for foreign XCCDF rules.
7. Define whether one Crystal Forge policy may intentionally back several imported source rules while preserving distinct XCCDF rule identities.
8. Define standard parameterization rules for converting policy configuration fields to XCCDF `Value` elements.
9. Define evidence attachment packaging and result signatures.
10. Select the exact third-party compatibility test versions and fixtures.

## 28. Normative references

- NIST IR 7275 Revision 4, *Specification for the Extensible Configuration Checklist Description Format (XCCDF) Version 1.2*.
- XCCDF 1.2 XML Schema.

## 29. Informative references

- DISA STIG Viewer 3.x User Guide.
- Current Crystal Forge compliance bundle API and persistence model.
- Current Crystal Forge deployment policy model and Nix policy evaluator.
- Current Crystal Forge Nix STIG module implementation.

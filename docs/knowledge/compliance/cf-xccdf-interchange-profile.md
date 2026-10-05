---
type: Design Specification
title: "CF-XCCDF Interchange Profile (retained design handoff)"
description: "Points to the retained v0.1 draft that defines how Crystal Forge imports and exports compliance bundles and policies as XCCDF 1.2 XML with a Crystal Forge extension, including conformance classes, round-trip, trust, and open decisions."
tags:
  - crystal-forge
  - xccdf
  - compliance
  - interchange
  - stig
implementation_status: partial
generated:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-03T22:58:48-05:00
sources:
  - id: origin
    resource: "Crystal Forge repository file docs/design/CrystalForge/docs/crystal-forge-xccdf-interchange-profile-v0.1.md at commit 3b23d36f"
    title: "Crystal Forge XCCDF Interchange Profile"
---
# CF-XCCDF Interchange Profile (retained design handoff)

> **Status:** partial. The retained document is a "Design draft for implementation review" (CF-XCCDF version 0.1, dated 2026-07-31). The server parses and exports CF-XCCDF, and the operator guide records tested limits: Levels A to C are claimed and Level D generic SCAP execution is not. Which of the profile's requirements hold has not been verified in full.

## Retained file

[crystal-forge-xccdf-interchange-profile-v0.1.md](../../design/CrystalForge/docs/crystal-forge-xccdf-interchange-profile-v0.1.md) (1082 lines, 29 numbered sections). The directory `docs/design/CrystalForge/uploads/` holds a byte-identical copy (`crystal-forge-xccdf-interchange-profile-v0.1.md`) and a copy of the repository `CLA.md`. Both stay in place. The `docs/` copy is the one that `schemas/cf-xccdf-1/PROVENANCE.md` names as the normative source of the CF-XCCDF schema.

## What the document specifies

The format has two purposes. A Crystal Forge export must import into another Crystal Forge installation without losing policy definitions or intended behavior. The same file must remain a conforming XCCDF 1.2 benchmark for standards-based viewers that ignore Crystal Forge extensions. Crystal Forge does not define a competing top-level format. It adds a small XML extension in the namespace `urn:crystal-forge:xccdf:1` for executable policy semantics.

Key design principles (Section 3):

- XCCDF stays authoritative for standard benchmark content. Crystal Forge metadata MUST NOT replace titles, descriptions, severities, identifiers, profiles, checks, fixes, values, or result fields.
- One Crystal Forge policy version is one XCCDF `Rule`. A multi-expression policy stays one `Rule`.
- A bundle is a baseline, not a sealed assignment. An assignment may exclude, add, and override values. Published versions stay unchanged and local changes are assignment overlays.
- Publication immutability is separate from tailoring. The profile does not use `prohibitChanges` for immutability. It uses version identities, digests, signatures, and storage rules.
- Standard consumers must get useful content without Crystal Forge extensions.

Section list:

1. Purpose
2. Normative language
3. Design principles
4. Scope
5. Terminology
6. Conformance classes
7. XML namespaces and checking systems
8. Portable artifact
9. Core XCCDF mapping
10. Benchmark representation
11. Policy representation
12. Current policy-type encodings
13. Policy phases and enforcement
14. Dependencies and non-global NixOS modules
15. Identifiers and framework mappings
16. Bundle assignment and tailoring
17. Round-trip requirements
18. Import behavior
19. Export behavior
20. Assessment result export
21. Trust and security
22. Compatibility promise
23. Validation and test suite
24. Required Crystal Forge data-model changes
25. Non-normative complete rule example
26. Decisions captured by this draft
27. Open decisions before version 0.2
28. Normative references
29. Informative references

## Implementation status and evidence

- Routes for XCCDF preview, import, bundle-version export, and assignment export exist in `packages/default/crates/cf-server/src/bin/server.rs` (`/api/v1/compliance/xccdf/preview`, `/api/v1/compliance/xccdf/import`, `/api/v1/compliance/bundle-versions/:version_id/xccdf`, `/api/v1/compliance/assignments/:assignment_id/xccdf`).
- `packages/default/crates/cf-server/src/queries/compliance_interchange.rs` exists, and migrations `0197_compliance_versioning.sql` and `0203_policy_bundle_trust_state.sql` add versioning and trust state.
- Section 16.5 recommends three export choices: canonical bundle, assignment tailoring, and effective benchmark. The operator guide says the implemented exports are the canonical bundle-version export and the effective derived benchmark for an assignment, and that XCCDF `Tailoring` is not the implemented path. The profile marks Tailoring as MAY, so this is a gap, not a conflict.
- Assessment result export (Section 20) and the Section 27 open decisions were not checked.

## Related concepts

- [CF-XCCDF interchange operator guide](cf-xccdf-interchange-operator-guide.md): implemented server/API behavior and tested limits.
- [Assignments, overlays, and report-only enforcement](assignments-and-report-only-enforcement.md): effective-set resolution that Section 16 describes.
- [STIG module system](stig-modules.md): NixOS-side STIG controls.
- [Schema provenance](../references/schema-provenance.md): the CF-XCCDF schema and its pinned XCCDF 1.2.1 schema set.
- [TASK-412 implementation records](../historical/task-412-implementation-records.md): the transactional trust and publication work for this profile.

---
type: Operator Guide
title: "CF-XCCDF Compliance Interchange Operator Guide"
description: "Explains operating CF-XCCDF compliance interchange: bundle and policy version lineage, importing foreign STIG/XCCDF and CF-XCCDF, trust and publication, XCCDF export, policy JSON/TOML interchange, and the tested compatibility limits."
tags:
  - crystal-forge
  - xccdf
  - compliance
  - interchange
  - stig
implementation_status: implemented
generated:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-03T22:56:07-05:00
sources:
  - id: origin
    resource: "Crystal Forge repository file docs/operator/compliance-interchange.md at commit 3b23d36f"
    title: "CF-XCCDF Compliance Interchange Operator Guide"
---
# CF-XCCDF Compliance Interchange Operator Guide

This guide describes the implemented server/API behavior in this branch. It does
not imply that every CF-XCCDF design-draft feature is available in the UI.

> **Status:** implemented as server/API behavior, as the guide itself states. The routes `/api/v1/compliance/xccdf/preview`, `/api/v1/compliance/xccdf/import`, `/api/v1/compliance/bundle-versions/:version_id/xccdf`, and `/api/v1/compliance/assignments/:assignment_id/xccdf` exist in `packages/default/crates/cf-server/src/bin/server.rs`. The guide says not every design-draft feature has a UI. The assignment overlay and report-only text moved to [assignments, overlays, and report-only enforcement](assignments-and-report-only-enforcement.md).

## Version and lineage semantics

A bundle lineage is the logical bundle identity. Each immutable bundle revision
has its own version ID and contains an ordered snapshot of policy version IDs.
The same distinction applies to policy lineages and policy versions.

`current` is a selection/pointer concept: the catalog may show the current
published revision by default, while an operator can explicitly select another
revision. `publication_state` is lifecycle state: draft revisions are editable;
accepted revisions are published and immutable. A revision is not changed merely
because a newer revision becomes current.

Exports and assignments use an exact bundle version ID. Do not infer a revision
from the bundle name, display order, or the newest row returned by a list query.
To edit a published revision, create a derived draft first.

## Importing foreign STIG/XCCDF

The server owns parsing and validation. The browser is not the authoritative XML
parser.

1. As an administrator, call `POST /api/v1/compliance/xccdf/preview` with the
   XML or supported ZIP package in the `file` multipart field.
2. Review benchmark metadata, profiles, rules, identifiers, checks, fixes,
   source digest, fidelity, warnings, and blocking diagnostics.
3. Select the benchmark/profile/rules and provide an import plan.
4. Call `POST /api/v1/compliance/xccdf/import` with the same file and plan.
5. The server reparses the upload, verifies the plan and source digest, and
   commits the import atomically.

Foreign rules are preserved as imported requirements. Unsupported checks are not
turned into invented Nix expressions. Imported content starts as draft,
disabled, untrusted, and unassigned. Import does not activate policies, evaluate
expressions, install modules, or assign a bundle.

The original package bytes and source provenance are retained. ZIP processing
selects an XCCDF entry server-side and records the selected entry and hashes.

## Importing CF-XCCDF

CF-native imports reconcile by portable version identity and semantic digest:

- An identical version is reused.
- A different version ID in the same lineage creates a new version.
- An existing immutable identity with a different digest is a blocking conflict.
- Titles, names, slugs, and local database IDs are not sufficient for identity.

Resolve conflicts explicitly. The importer never silently overwrites an
immutable version.

## Trust and publication

Import and trust are separate operations. Review executable policy XML,
dependencies, preserved source content, and fidelity warnings before trusting.

Publishing an accepted bundle revision freezes its metadata, ordered membership,
policy-version references, and digest. Included draft policy versions may be
published atomically when the publish request opts into that behavior.

Published revisions are immutable. Create a new draft derived from the published
revision for changes. Policy trust and publication are currently available via
the server API; do not assume a complete UI workflow for every lifecycle action.

## XCCDF export

Canonical baseline export is exact-revision scoped:

```text
GET /api/v1/compliance/bundle-versions/:version_id/xccdf
```

It returns one XCCDF 1.2 `Benchmark`, one baseline `Profile`, and one `Rule`
per exported policy version. The baseline export does not include local
assignment state.

To export the resolved assignment, including exclusions, additions, and applied
configuration overrides, use:

```text
GET /api/v1/compliance/assignments/:assignment_id/xccdf
```

This is an effective derived benchmark export. It resolves the assignment first
and refuses export when the effective set has a conflict. Do not describe this
as an XCCDF `Tailoring` document: Tailoring is not the implemented export path
for these assignment overlays.

The writer emits XCCDF through typed server structures. It preserves supported
CF policy configuration, standard metadata, imported checks/fixes where valid,
and opaque source content supported by the model. Invalid imported check or fix
content is reported as a validation error rather than silently rewritten.

## Policy JSON/TOML interchange

Policy JSON/TOML endpoints are server-side canonical adapters. They support the
native policy types and multi-rule custom checks without flattening them to one
expression. The legacy simplified single-expression custom-check shape is
accepted and normalized. Imported policies remain draft until explicitly
activated or published.

## Compatibility and tested limits

The implemented and tested compatibility claim is:

- **Level A:** XCCDF 1.2 XML is produced and standard fields are available to
  compatible XCCDF viewers.
- **Level B:** Standard human-readable rule content can be used as a checklist
  where the consumer supports it.
- **Level C:** Crystal Forge can parse its supported CF-XCCDF extension and
  reconstruct supported policy/bundle data by identity and digest.

Level D generic SCAP execution is **not claimed**. CF-XCCDF policy checks are
not automatically OVAL/OCIL checks, and no generic scanner execution guarantee
is made.

The branch tests server parsing, XML writing, schema-shaped exports, identity and
digest handling, policy JSON/TOML round trips, assignment resolution, effective
set digests, and effective assignment export. Compatibility with a particular
third-party viewer or STIG Viewer release has not been established here unless a
separate test record names that exact version. XCCDF validity alone does not
guarantee acceptance of Crystal Forge extension content by every product.

The server enforces bounded XML/ZIP processing, rejects DTD/external-entity and
archive traversal conditions, and reports structured blocking diagnostics. The
configured upload and parser limits are implementation limits, not a promise
that arbitrary large STIG packages are supported.

## Related concepts

- [Assignments, overlays, and report-only enforcement](assignments-and-report-only-enforcement.md): the former "Assignments and overlays" section of this guide.
- [CF-XCCDF interchange profile design](cf-xccdf-interchange-profile.md): the normative design draft this guide implements in part.
- [STIG module system](stig-modules.md): NixOS-side STIG controls.
- [Schema provenance](../references/schema-provenance.md): where the CF-XCCDF and OSCAL schemas come from.

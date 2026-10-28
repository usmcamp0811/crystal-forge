---
type: Reference
title: "CF-XCCDF and OSCAL POA&M Schema Provenance (retained files)"
description: "Points to the retained provenance notes for the CF-XCCDF v0.1 schema and the unmodified NIST OSCAL POA&M v1.1.2 JSON and XSD schemas, including their release URLs and SHA-256 digests."
tags:
  - crystal-forge
  - schemas
  - xccdf
  - oscal
  - provenance
generated:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-03T22:58:48-05:00
sources:
  - id: s1
    resource: "Crystal Forge repository file schemas/cf-xccdf-1/PROVENANCE.md at commit 3b23d36f"
    title: "CF-XCCDF v0.1 schema provenance"
  - id: s2
    resource: "Crystal Forge repository file schemas/oscal-1.1.2/poam-provenance.txt at commit 3b23d36f"
    title: "OSCAL POA&M v1.1.2 schemas provenance"
---
# CF-XCCDF and OSCAL POA&M Schema Provenance (retained files)

## Retained files

| File | Covers |
| --- | --- |
| [schemas/cf-xccdf-1/PROVENANCE.md](../../../schemas/cf-xccdf-1/PROVENANCE.md) | `cf-xccdf-1.xsd`, maintained by Crystal Forge for the frozen extension namespace `urn:crystal-forge:xccdf:1`. |
| [schemas/oscal-1.1.2/poam-provenance.txt](../../../schemas/oscal-1.1.2/poam-provenance.txt) | The unmodified NIST OSCAL POA&M v1.1.2 JSON and XSD schemas. |

Both files stay beside the schemas they describe. Tests and packages read the `schemas/` tree by path.

## CF-XCCDF schema

- `cf-xccdf-1.xsd` is maintained by Crystal Forge. Its normative source is the CF-XCCDF v0.1 profile (see [CF-XCCDF interchange profile](../compliance/cf-xccdf-interchange-profile.md)).
- The XCCDF 1.2.1 schema set is packaged as `packages/xccdf-1-2-schemas`. It copies the XCCDF, CPE language, and XML namespace schemas shipped by the pinned OpenSCAP package into a self-contained Nix output.
- The upstream XCCDF schema identifies NIST IR 7275 Revision 4 and the schema date 2012-02-23.
- Crystal Forge does not fetch schemas at runtime.

## OSCAL POA&M schemas

The files are unmodified NIST release assets of OSCAL v1.1.2 (release page `https://github.com/usnistgov/OSCAL/releases/tag/v1.1.2`):

| Asset | SHA-256 |
| --- | --- |
| `oscal_poam_schema.json` | `906725163d767036c6189aec51252109b203214e121fc1acaff494b4d2dfbc04` |
| `oscal_poam_schema.xsd` | `4de13f26b9c0007504029daf3dd3538250c684139b9fec516d23789110fe05a1` |

## Related concepts

- [CF-XCCDF interchange profile](../compliance/cf-xccdf-interchange-profile.md): the normative source of the CF-XCCDF schema.
- [CF-XCCDF interchange operator guide](../compliance/cf-xccdf-interchange-operator-guide.md): operator behavior, including bounded XML and ZIP processing.
- [CVE/POA&M evidence continuity design](../poam/cve-poam-evidence-continuity-design.md): the POA&M register that OSCAL exports represent.

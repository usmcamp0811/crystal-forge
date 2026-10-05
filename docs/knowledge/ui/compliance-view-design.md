---
type: Design Specification
title: "Compliance View Design Review (retained design handoff)"
description: "Points to the retained v0.1 review draft of the production /compliance route (catalog, requirement coverage, systems matrix, evidence drawer, POA&M integration, export), its 32 gaps, 15 pending decisions, and its status."
tags:
  - crystal-forge
  - compliance
  - web-ui
  - design-review
  - poam
implementation_status: partial
generated:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-03T22:58:47-05:00
sources:
  - id: s1
    resource: "Crystal Forge repository file docs/design/CrystalForge/docs/crystal-forge-compliance-design/compliance-view-design-v0.1.md at commit 3b23d36f"
    title: "Crystal Forge Compliance View: Architecture, data provenance, POA&M integration, and consistency contract"
  - id: s2
    resource: "Crystal Forge repository file docs/design/CrystalForge/docs/crystal-forge-compliance-design/README.md at commit 3b23d36f"
    title: "Crystal Forge Compliance architecture review, v0.1"
  - id: s3
    resource: "Crystal Forge repository file docs/design/CrystalForge/docs/crystal-forge-compliance-design/verification.md at commit 3b23d36f"
    title: "Verification record"
---
# Compliance View Design Review (retained design handoff)

> **Status:** partial. The retained document is an evidence-labelled review draft, not an implementation plan. Its AS-BUILT sections describe the `/compliance` route at the pinned source `931e36229ed548b0c62b560fc99f9415e3829cef` (MR !329). Its proposed contracts (Section 21), workflow contracts (Section 22), regression matrix (Section 23), and decisions (Section 25) are not approved or implemented by that document. This pointer is a navigation and status record. The retained files are the authoritative text.

## Retained files

| File | Role |
| --- | --- |
| [compliance-view-design-v0.1.md](../../design/CrystalForge/docs/crystal-forge-compliance-design/compliance-view-design-v0.1.md) | Main review document (1590 lines, 26 numbered sections, 14 inline Mermaid diagrams). |
| [README.md](../../design/CrystalForge/docs/crystal-forge-compliance-design/README.md) | Bundle contents table, reading order, rendering notes, and status (documentation only; no application tests run). |
| [verification.md](../../design/CrystalForge/docs/crystal-forge-compliance-design/verification.md) | What was inspected at the pinned SHA, what was not executed, and the head pipeline observation (failed, cause not investigated). |

The same directory holds machine-readable registers (`gap-register.json`, `regression-matrix.json`, `decision-register.json`, `cross-view-contract-ledger.json`, `source-manifest.json`, `diagram-manifest.json`), Mermaid sources under `diagrams/`, and checksum files. Nix packages and checks read the design handoff tree by path, so it stays in place.

## What the document specifies

The document covers the production `/compliance` route: bundle catalog, revision selection, requirement coverage, systems matrix, evidence drawer, assignment maintenance, finding-origin POA&M actions, bundle POA&M lists, the common POA&M detail tray, and evidence export. Each claim carries one evidence label: AS-BUILT, EXISTING SPEC, PROPOSED, or UNVERIFIED.

Section list:

1. Purpose, evidence, and revision boundary
2. Existing design contracts and conflicts
3. Surface and component model
4. Identity and state model (identity dictionary; orthogonal state dimensions)
5. UI-to-API contract inventory
6. Persistence and provenance map (GET evidence is not purely read-only)
7. Bundle versions, assignments, and requirement coverage (requirement baseline is independent of the selected implementation; assignment references do not establish remediation coverage)
8. Evidence source selection and authority (composite digest mismatch; evaluation-attempt fallback is not deployed evidence; latest row versus latest valid row; policy thresholds versus individual CVEs)
9. Count units, scores, and misleading clean states
10. Navigation, request state, and refresh boundaries
11. Finding-origin POA&M creation and linking
12. Common POA&M detail, families, and entry-point consistency
13. Verification, closure, and reopening
14. Waivers, CVE acceptance, and assignment exceptions
15. Evidence export and report integrity
16. Import, bundle maintenance, and assignment side effects
17. Authorization, errors, loading, and unavailable states
18. Source-level design parity register
19. Performance and query behavior
20. Consolidated gap register (32 open gaps)
21. Proposed shared contracts
22. End-to-end workflow contracts for review
23. Verification strategy and regression matrix (80 proposed, unexecuted scenarios)
24. Cross-view consistency ledger
25. Decision register (CPD01 to CPD15, all pending)
26. Sources, verification limits, and artifact record

Key points a reader needs before opening it:

- Policy findings, exact-CVE findings, policy waivers, and CVE accepted risk are separate families. The document says they must not be unified only because both use the word "accepted".
- Mapping coverage, observed result, disposition, and remediation lifecycle are separate dimensions.
- A bundle version selects membership. It is not a deployment or observation identity.

## Implementation status and evidence

- The route exists: `/compliance` is declared in `packages/web-ui/src/routes.rs`, and the view is `packages/web-ui/src/views/compliance.rs`.
- Server routes for bundle versions, requirement coverage, assignments, and finding waivers exist in `packages/default/crates/cf-server/src/bin/server.rs`.
- The document is pinned to `931e3622`. The current tree was not re-compared against its 32 gaps or its AS-BUILT statements. Mark every gap as unverified until a verification pass runs.
- The document records that the Systems and CVEs drafts inspected `58006084`. Its statement that the three drafts describe one application source state applies only to that pin.

## Related concepts

- [Cross-view contract ledger](cross-view-contract-ledger.md): the open Systems, CVEs, and Compliance worksheet that this design feeds.
- [CVEs view design review](cves-view-design.md): the companion draft for the fleet `/cves` page.
- [CVE/POA&M evidence continuity design](../poam/cve-poam-evidence-continuity-design.md): the approved contract for CVE evidence and POA&M continuity.
- [Compliance UI redesign spec](compliance-ui-redesign-spec.md): the earlier specification for the bundle table and drawer.
- [TASK-433 design parity review](../historical/task-433-design-parity-review.md): the parity review this document cites.
- [Assignments, overlays, and report-only enforcement](../compliance/assignments-and-report-only-enforcement.md): server-side assignment semantics.

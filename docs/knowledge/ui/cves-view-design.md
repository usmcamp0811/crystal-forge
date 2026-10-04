---
type: Design Specification
title: "CVEs View Design Review (retained design handoff)"
description: "Points to the retained v0.1 review draft of the fleet /cves page (grouped and flat inventory, filters, statistics, fleet drawer, environment triage, host overrides, exact-CVE POA&M lifecycle), its 20 gaps and 13 decisions, and the precedence of the continuity design over it."
tags:
  - crystal-forge
  - cves
  - web-ui
  - design-review
  - triage
implementation_status: partial
generated:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-03T22:58:48-05:00
sources:
  - id: s1
    resource: "Crystal Forge repository file docs/design/CrystalForge/docs/crystal-forge-cves-design/cves-view-design-v0.1.md at commit 3b23d36f"
    title: "Crystal Forge CVEs View: Architecture, data provenance, and consistency contract"
  - id: s2
    resource: "Crystal Forge repository file docs/design/CrystalForge/docs/crystal-forge-cves-design/README.md at commit 3b23d36f"
    title: "Crystal Forge CVEs View design review"
  - id: s3
    resource: "Crystal Forge repository file docs/design/CrystalForge/docs/crystal-forge-cves-design/verification.md at commit 3b23d36f"
    title: "Verification record"
---
# CVEs View Design Review (retained design handoff)

> **Status:** partial. The retained draft is time-pinned to `58006084aa699b84bcb1d02d6f911d4d4ee94ea3`. Its AS-BUILT text and diagrams are evidence of the inspected source, not the current product contract. The owner-approved [CVE/POA&M continuity design](../poam/cve-poam-evidence-continuity-design.md) (Section 29) supersedes the draft's retained-artifact CVE gate, unchanged-lineage verification, and bidirectional scheduled-membership equality. The draft says TASK-326.2.2 is implementing the new contract and that it did not verify that work.

## Retained files

| File | Role |
| --- | --- |
| [cves-view-design-v0.1.md](../../design/CrystalForge/docs/crystal-forge-cves-design/cves-view-design-v0.1.md) | Main review document (1532 lines, 26 numbered sections, 12 inline Mermaid diagrams). |
| [README.md](../../design/CrystalForge/docs/crystal-forge-cves-design/README.md) | Review order, bundle contents, and a table of the 12 diagrams with their AS-BUILT, EXPLANATORY, EXISTING PROPOSAL, or PROPOSED labels. |
| [verification.md](../../design/CrystalForge/docs/crystal-forge-cves-design/verification.md) | The 64 artifact checks run, and the checks not run (no Rust, SQLx, database, VM, Playwright, or live checks; Mermaid rendering unverified). |

The directory also holds `diagrams/*.mmd`, `source-manifest.json`, `artifact-checks.json`, and `checksums.sha256`. Nix packages and checks read the design handoff tree by path, so it stays in place.

## What the document specifies

The document covers the fleet `/cves` page: package groups, the flat table, filters, statistics, export, fleet rescan, the fleet inventory drawer, the nested triage editor, host overrides, scan admission, and the exact-CVE POA&M lifecycle. It keeps current code, existing specifications, and proposed behavior separate.

Section list:

1. Purpose, evidence, and review boundary
2. Existing specifications and conflicts
3. Surface and component model
4. Terminology and independent state dimensions
5. Persistence and identity relationships
6. Data producers and source map
7. Inventory selection and authority (Current, Scheduled deployment target, Historical)
8. Count contract
9. HTTP contract and bounds
10. Page state, navigation, and refresh
11. Fleet drawer and navigation identity
12. Triage decisions and host-override precedence
13. Triage transaction, concurrency, and retry contract
14. Exact-CVE POA&M verification and lifecycle
15. Normative target: evidence continuity across deployments
16. Loading, error, empty, and stale-state contracts
17. Design-reference comparison
18. Performance and query behavior
19. Authorization, privacy, and export
20. Consolidated gap register (20 gaps)
21. Proposed target contract
22. End-to-end workflow examples
23. Test coverage and regression matrix (56 scenarios)
24. Change boundaries and compatibility
25. Decision register (13 decisions)
26. Source index and verification record

Reading order recommended by the README: Section 2, then Sections 7 and 8 (source selection and count units), Sections 10 to 14 (refresh, host overrides, transactions, current verification rule), Section 15 (the continuity proposal), Sections 20 and 25 (gaps and decisions), and Section 23 (regression scenarios).

## Implementation status and evidence

- The fleet CVE triage behavior that the draft audits exists. The server bounds (`MAX_FLEET_CVE_ENVIRONMENTS = 100`, `MAX_FLEET_CVE_SUBJECTS = 1_000`, `MAX_FLEET_CVE_BATCH_IDENTITIES = 100`) and conflict codes such as `cve_evidence_changed` are in `packages/default/crates/cf-server/src/services/poam.rs`. The batch limit `MAX_CVE_BATCH_PAIRS = 100` is in `packages/web-ui/src/views/cves.rs`.
- The baseline-generation verification rule and the read-only Current inventory that the draft describes are superseded. Migrations `0279` to `0284` (continuity, closed-baseline replay history, occurrence precedence, moved-environment history) show that the continuity work has begun in code. The draft's gaps and diagrams were not re-checked against the current tree.

## Related concepts

- [Fleet CVE triage operator guide](../cves/fleet-cve-triage.md): operator-facing behavior of the fleet drawer.
- [CVE POA&M verification and closure](../poam/cve-poam-verification-and-closure.md): the current verification contract in operator terms.
- [CVE/POA&M evidence continuity design](../poam/cve-poam-evidence-continuity-design.md): the superseding approved design.
- [Cross-view contract ledger](cross-view-contract-ledger.md): CVEs input to the joint review.
- [Compliance view design review](compliance-view-design.md): the companion draft for `/compliance`.

---
type: Design Specification
title: "Systems view architecture and consistency contract"
description: "Summarizes the Systems view architecture, data provenance, and consistency contract (version 0.3), its section map, owner decisions D1 to D8, and staged slices SC1 to SC5."
tags:
  - crystal-forge
  - web-ui
  - systems
  - cves
  - design-handoff
implementation_status: partial
generated:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-03T22:54:52-05:00
sources:
  - id: s1
    resource: "Crystal Forge repository file docs/design/CrystalForge/docs/crystal-forge-systems-design/systems-view-design-v0.1.md at commit 3b23d36f"
    title: "Crystal Forge Systems View"
---
# Systems view architecture and consistency contract

This pointer concept describes [systems-view-design-v0.1.md](../../design/CrystalForge/docs/crystal-forge-systems-design/systems-view-design-v0.1.md), a 1,314-line document titled "Crystal Forge Systems View: Architecture, data provenance, and consistency contract". It is part of the design handoff tree `docs/design/CrystalForge/` and stays at its original path. The filename keeps `v0.1` for stable links. The internal version is 0.3 (Claude UI design authority and staged implementation contract), review date 2026-09-23.

## What it specifies

The document describes the Systems list, its preview panel, and all eight System Detail tabs. It defines consistency boundaries with Scanning, fleet CVEs, Config evidence, and POA&M. The central problem is that several screens describe a system's current security state through different identity checks and different counting methods. The document has two purposes: it records the implementation at a pinned source revision, and it proposes an explicit contract for behavior that is missing, inconsistent, or undecided. Evidence labels (AS-BUILT, EXISTING SPEC, USER REQUIREMENT, AGREED / SC1, AGREED / LATER, WORKING LIFECYCLE MAPPING, PROPOSED, UNVERIFIED) separate current fact from proposal.

## Section map

| Sections | Topic |
| --- | --- |
| 1 | Purpose, status, the 2026-09-23 decision update, the UI design authority rule (the owner's Claude design is the UI source of truth), evidence labels, review boundary, and reading map. |
| 2 | Existing specifications and conflicts between them, and proposed document ownership. |
| 3 to 4 | Screen and route model, navigation state, identity terms that must stay separate, and data ownership. |
| 5 to 9 | AS-BUILT source map, the several ways Current identity is resolved, what creates retained-generation authority, the reported inconsistency (screenshot diagnosis), and count semantics. |
| 10 to 11 | Revision defaults and explicit selection, and the proposed shared read model (facts separate from capabilities, read-only evidence without complete deployment proof). |
| 12 to 13 | Per-screen behavior and gaps (Systems list, preview, header and Overview, Deploy, History, Logs, Config, CVEs, Hardening, Compliance and POA&M) and the structural comparison with the design examples. |
| 14 to 17 | Refresh and concurrency, authorization and mutation safety, loading, error, empty, and partial states, and performance. |
| 18 to 21 | Consolidated gap register (G01 to G20), verification contract and regression matrix, staged implementation (slices SC1 to SC5 and optional agent scanning), and the live diagnosis needed for one reported system. |
| 22 | Owner decisions D1 to D8 and implementation scope. |
| 23 | Acceptance criteria for the architecture contract. |
| Appendix A, B | Screenshot evidence and the source index S01 to S38. |

## Key decisions recorded in Section 22

- **D1, D2.** A uniquely mapped completed schema-1 scan can be shown without complete deployment proof. SC1 first made it read-only until trusted server reconciliation retained the observed generation. The continuity design later superseded that gate for CVE mutation. Unmapped running output stays unmapped, with no automatic flake-head fallback.
- **D3.** A CVE count means distinct canonical CVE IDs. A finding count means distinct CVE and package pairs. A scan occurrence count is a third unit.
- **D4.** The header stays scoped to the running configuration.
- **D5.** Refresh follows intent. Current re-resolves to the reported running target. A newly evaluated commit only adds a browsing choice.
- **D6.** Findings keep a stable identity (`system_id + canonical_cve_id + canonical_package_name`). Edits capture their start context. Later slices implement continuity.
- **D7.** Disappearance of a finding yields "Candidate remediated" and then the existing "Awaiting verification" state. Formal closure needs authoritative verification. Automatic closure is a separate policy choice.
- **D8.** Document precedence for SC1 and the work remaining before SC5.

## Staged implementation (Section 20)

| Slice | Outcome |
| --- | --- |
| SC1 | Current-first CVE browsing, target and scan selection, trusted reconciliation of external activation. |
| SC2 | Inventory and count consistency, running header distinct CVEs, full pagination. |
| SC3 | Host triage workflow. |
| SC4 | Environment decisions and overrides. |
| SC5 | Cross-revision continuity and completion. |
| Later | Optional agent scanning (observational Vulnix results for unmapped output). |

## Implementation status

> **Status:** partial. The document is part audit, part approved design, part proposal. SC1 target and scan selection shipped. The continuity design superseded parts of the SC1 gate. The status of SC2 to SC5 was not checked.

Evidence checked: `mapped_running` handling in `packages/default/crates/cf-server/src/queries/cves.rs` and `packages/web-ui/src/views/system_detail.rs`, the `binding_origin` migrations `0277` and `0278`, and the continuity migrations `0279` to `0281` with `view_current_cve_authority`. The AS-BUILT statements describe source at commits `58006084` and `327d03b6` and were not rechecked against the current head.

## Related concepts

- [Design handoff for Systems design and System Detail CVEs](systems-design-handoff-bundle.md)
- [SC1 contract](system-cves-sc1-contract.md)
- [SC1 handoff, validation, and design review](system-cves-sc1-handoff-and-validation.md)
- [Systems deployment progress, real activity, and rollback specification](systems-deployment-progress-spec.md)
- [Sidebar badges versus the notification bell](alerts-and-notifications-decision.md)

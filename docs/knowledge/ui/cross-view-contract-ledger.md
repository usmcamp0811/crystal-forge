---
type: Design Specification
title: "Cross-View Contract Ledger (retained design handoff)"
description: "Points to the retained open worksheet of 22 contracts (CPC01 to CPC22) that Systems, fleet CVEs, and Compliance views must agree on, none of which is approved; open it to see the unresolved cross-view questions."
tags:
  - crystal-forge
  - compliance
  - cves
  - web-ui
  - design-review
implementation_status: proposed
generated:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-03T22:58:48-05:00
sources:
  - id: origin
    resource: "Crystal Forge repository file docs/design/CrystalForge/docs/crystal-forge-compliance-design/cross-view-contract-ledger-v0.1.md at commit 3b23d36f"
    title: "Crystal Forge: Cross-view Contract Ledger"
---
# Cross-View Contract Ledger (retained design handoff)

> **Status:** proposed. The retained worksheet states: "Open. No cross-view decision is approved by this worksheet." Every row's joint decision, required change, proof, and approval cell is `Pending` or `Not approved`. The retained file is the authoritative text. This pointer only records scope and status.

## Retained file

[cross-view-contract-ledger-v0.1.md](../../design/CrystalForge/docs/crystal-forge-compliance-design/cross-view-contract-ledger-v0.1.md) (117 lines, version 0.1, one Mermaid flowchart). A machine-readable copy is `cross-view-contract-ledger.json` in the same directory.

## What the document specifies

The ledger compares three drafts: the Compliance draft at `931e3622`, and the Systems and CVEs drafts at `58006084`. A direct comparison showed documentation additions only between those commits. "Must agree" means equal identities and scopes must yield consistent semantics. "Decision required" stays open even when the implementation already makes a choice.

It contains 22 contract rows. Each row has a treatment (must agree, intentional distinction, decision required, or verification required), a Systems input, a fleet CVEs input, a Compliance finding, and a joint review question:

| Group | Rows |
| --- | --- |
| Identity and selection | CPC01 finding identity, CPC02 target and revision selection |
| Evidence authority | CPC03 read fallback versus write authority, CPC04 complete versus enforced policy context, CPC05 newest report versus last valid report, CPC06 threshold policy versus exact CVE |
| Scope and units | CPC07 authorization before aggregation, CPC08 unassigned or unavailable systems, CPC09 count units, CPC10 coverage, result, and remediation |
| Dispositions | CPC11 risk acceptance and waiver, CPC12 host override and environment default, CPC13 baseline and cross-deployment continuity, CPC14 bundle and assignment association |
| POA&M behavior | CPC15 typed assignee and required metadata, CPC16 common plan capabilities, CPC17 verification and closure, CPC18 committed errors and retries |
| Refresh and reporting | CPC19 invalidation and unsaved drafts, CPC20 history and return navigation, CPC21 export source, scope, and time, CPC22 finding materialization and count completeness |

The review order is: identity and visibility, then selection, authority, and completeness, then units and decision types, then verification and continuity, then navigation, invalidation, history, and export. The ledger asks reviewers to attach one adversarial fixture to each accepted rule.

## Implementation status and evidence

No row is approved, so no row is a requirement. Some rows describe behavior that the code may already implement (for example CPC13, where the ledger says the shared exact-CVE verifier returned MISSING when the deployed identity changed; the continuity design supersedes that rule). Migrations `0279` to `0284` in `packages/default/crates/cf-server/migrations` add the continuity behavior. The ledger was not re-checked against them.

## Related concepts

- [Compliance view design review](compliance-view-design.md): Sections 8 to 15 and 20 to 25 of that review feed this ledger.
- [CVEs view design review](cves-view-design.md): the fleet CVEs input column.
- [CVE/POA&M evidence continuity design](../poam/cve-poam-evidence-continuity-design.md): CPC13 and the continuity decisions.

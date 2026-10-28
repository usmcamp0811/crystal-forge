---
type: Design Specification
title: "SC1 handoff, validation, and design review"
description: "Describes the SC1 agent prompt, manual validation guide, implementation notes, and Claude design review note, with their status and precedence; open it to find SC1 validation steps."
tags:
  - crystal-forge
  - web-ui
  - systems
  - cves
  - validation
implementation_status: partial
generated:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-03T22:54:52-05:00
sources:
  - id: s1
    resource: "Crystal Forge repository file docs/design/CrystalForge/docs/crystal-forge-systems-design/system-cves-chunk-1-agent-prompt.md at commit 3b23d36f"
    title: "SC1 agent prompt"
  - id: s2
    resource: "Crystal Forge repository file docs/design/CrystalForge/docs/crystal-forge-systems-design/system-cves-chunk-1-manual-validation.md at commit 3b23d36f"
    title: "SC1 manual validation"
  - id: s3
    resource: "Crystal Forge repository file docs/design/CrystalForge/docs/crystal-forge-systems-design/system-cves-sc1-implementation.md at commit 3b23d36f"
    title: "SC1 implementation and verification notes"
  - id: s4
    resource: "Crystal Forge repository file docs/design/CrystalForge/fixtures/system-cves-sc1-review.md at commit 3b23d36f"
    title: "SC1 System Detail CVEs: Claude design review"
---
# SC1 handoff, validation, and design review

This pointer concept covers four companion documents of the SC1 contract. All four stay at their original paths. Three are in the design handoff tree `docs/design/CrystalForge/docs/crystal-forge-systems-design/`. One is a design fixture note in `docs/design/CrystalForge/fixtures/`. The tree stays in place because Nix packages, fixture seeding, and checks read it by path.

## Documents

| Retained file | What it is |
| --- | --- |
| [system-cves-chunk-1-agent-prompt.md](../../design/CrystalForge/docs/crystal-forge-systems-design/system-cves-chunk-1-agent-prompt.md) | A 159-line implementation-agent prompt (handoff revision 2). It states the base branch and inspected head, the UI authority and design-gap rule, eight required behaviors, starting points in `packages/web-ui` and `packages/default/crates/cf-server`, scope limits, preview and proof rules, Nix verification commands, and the handoff and stop conditions. |
| [system-cves-chunk-1-manual-validation.md](../../design/CrystalForge/docs/crystal-forge-systems-design/system-cves-chunk-1-manual-validation.md) | A manual validation guide for the owner. It lists design prerequisites, fixture roles (known Current target A with scan SA, undeployed target B with scan SB, mapped local activation, mapped A without retained proof, unmapped output, mapped target without scan, historical target), nine checks, and an approval record using Pass, Fail, Not verified, or Blocked: design gap. |
| [system-cves-sc1-implementation.md](../../design/CrystalForge/docs/crystal-forge-systems-design/system-cves-sc1-implementation.md) | Implementation and verification notes with status "in progress". It records the read and write boundaries, the `binding_origin` column on retained generations, background repair of up to 16 observed candidates every 60 seconds, the design comparison, and the blocked authoritative browser workflows (blocked by the shared design-targets prerequisite of TASK-440). |
| [system-cves-sc1-review.md](../../design/CrystalForge/fixtures/system-cves-sc1-review.md) | A design reference note for the Claude design review of SC1. It maps SC1 cases to design states in `SystemDetail.jsx` and `?sc1=` keys defined in `system-cves-sc1.js`, and describes the developer-only `cf-sc1-design-state` console event and the repeatable check `fixtures/system-cves-sc1-check.js` (1440 by 900 and 900 by 768 pixels, dark and light). |

## Status and precedence

> **Status:** partial. These documents are time-pinned to SC1 handoff revision 2 and commit `327d03b6`. The implementation notes and the Systems document state that the CVE/POA&M continuity design, Section 29, supersedes SC1's permanent retained-artifact gate, unchanged-generation verification, and historical-versus-current membership equality. TASK-326.2.2 is called "in progress" in the notes. Its status was not checked.

Evidence checked: the `binding_origin` migrations `0277` and `0278`, the repair and reconciliation code paths under `packages/default/crates/cf-server/src/queries/evaluation_snapshots.rs`, and the continuity migrations `0279` to `0281`. The 60-second repair cadence and the 16-candidate bound were not compared with the code.

## Related concepts

- [SC1 contract](system-cves-sc1-contract.md)
- [Design handoff for Systems design and System Detail CVEs](systems-design-handoff-bundle.md)
- [Systems view architecture and consistency contract](systems-view-design-specification.md)

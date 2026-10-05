---
type: Design Specification
title: "SC1 contract: System Detail CVEs target and scan selection"
description: "Summarizes the SC1 contract for System Detail CVEs target and scan selection, its section map, precedence note, and implementation evidence; open it before changing CVE tab target selection."
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
    resource: "Crystal Forge repository file docs/design/CrystalForge/docs/crystal-forge-systems-design/system-cves-chunk-1.md at commit 3b23d36f"
    title: "SC1: System Detail CVEs target and scan selection"
---
# SC1 contract: System Detail CVEs target and scan selection

This pointer concept describes [system-cves-chunk-1.md](../../design/CrystalForge/docs/crystal-forge-systems-design/system-cves-chunk-1.md), the contract for slice SC1 of the Systems design. The file is part of the design handoff tree `docs/design/CrystalForge/` and stays at its original path. It is the time-pinned TASK-326.2.1 handoff at commit `327d03b6` (handoff revision 3, decision date 2026-09-24). SC1 is not a pre-existing Backlog task ID. Its parent is the [Systems view architecture and consistency contract](systems-view-design-specification.md), Sections 1.0, 10, 11.4, and 22.

## What it specifies

An operator opens System Detail, then CVEs, and can tell which running or explicitly selected configuration a scan describes. The page must show matching results, an honest no-scan state, or an honest unmapped or unavailable state. It must never substitute flake-head evidence for an unmapped running system.

| Section | Content |
| --- | --- |
| 1 Outcome | The operator-visible goal and the stop condition. |
| 2 Controlling decisions | The default is Current with no automatic head substitution. A local activation describes switch origin, not CVE authority. Subsection 2.1 is the UI authority and design gate: the owner's Claude design is authoritative, no new banners, badges, panels, or controls, and a missing design state is reported as "Blocked: design gap". |
| 3 Scope | Included and excluded work. Local agent scanning, new scanner protocol, cross-generation continuity, header and count consolidation, and the Scanning false-clean fix are excluded. |
| 4 Target intent, source, and capability | The facts to distinguish (target intent, latest observation, running mapping, resolved target, selected source, proof, read capability, mutation capability). Subsections cover Current with full proof, Current with a uniquely mapped target, the unmapped, no-report, ambiguous, and no-scan states, and explicit revision browsing. |
| 5 Refresh, navigation, and drafts | Refresh re-resolves Current. Explicit targets survive reload and history. Responses are validated against the system and target. Mounted drafts are not silently retargeted. |
| 6 Security and compatibility invariants | Authorize the system before exposing source identity. Keep hidden and absent resources non-enumerating. GET must not run Nix, enqueue scans, persist a deployment, insert a retained generation, create a POA&M, or change a disposition. |
| 7 Starting source map | Modules, symbols, and browser checks to read first. |
| 8 Acceptance matrix | Cases SC1-01 to SC1-15 with required proof. |
| 9 Verification and handoff | Nix-based checks, the live preview and database rules, and the handoff content. |

## Precedence

> **Status:** partial. The retained file annotates its own precedence. The CVE/POA&M continuity design, Section 29, supersedes SC1's retained-artifact gate and deferred cross-revision behavior for Current CVE actions and verification. Acceptance cases SC1-02, SC1-03, SC1-08, SC1-13, and SC1-14 record the pinned SC1 gate and are not acceptance tests for TASK-326.2.2.

## Implementation status

Evidence checked: `mapped_running` handling in `packages/default/crates/cf-server/src/queries/cves.rs` and `packages/web-ui/src/views/system_detail.rs`, `cve_target` and `cve_mode` route parameters in `packages/web-ui/src/views/system_detail.rs`, the `binding_origin` migrations `0277` and `0278`, and the continuity migrations `0279` to `0281` with `view_current_cve_authority`. The browser checks named in Section 7 (`12ha-system-detail-cve-inventory-fallbacks`, `12h-system-detail-cves-grouped-justification`) live in `checks/web-ui/tests/integration-test.js`. The retained notes record the authoritative NixOS browser workflows as blocked at the time of writing. That status was not rechecked.

## Related concepts

- [Design handoff for Systems design and System Detail CVEs](systems-design-handoff-bundle.md)
- [SC1 handoff, validation, and design review](system-cves-sc1-handoff-and-validation.md)
- [Systems view architecture and consistency contract](systems-view-design-specification.md)

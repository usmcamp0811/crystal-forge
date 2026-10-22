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

## Selected evidence presentation

### Running-header evidence binding

The owner-authorized TASK-490.16 header repair uses the existing Current
inventory resolver independently of an explicit CVEs tab selection. The header
reuses the Current read when that target is selected. Historical inspection adds
one bounded Current read for the header. Both reads use the existing refresh
epochs. No new polling or mutation occurs.

A user CVE refresh or target selection also refreshes system detail in the same
epoch. This bounded pair of reads lets a mounted page recover after an external
activation. Until the new detail generation matches Current, the header stays
unavailable. The dependency does not increment a refresh nonce or restart the
Current resource. It cannot create a mismatch-retry loop or duplicate resolver
read. The existing Assurance B deployment keeps an A CVEs page mounted in a
second tab to verify this transition around the sole real activation.

The [Systems contract](../../design/CrystalForge/docs/crystal-forge-systems-design/systems-view-design-v0.1.md),
Section 9.2 and decision D3, defines header CVEs as distinct canonical CVE IDs.
The tab's findings remain canonical CVE/package pairs. The header uses the
unfiltered Current metadata `total_cves`, `cve_critical`, and `cve_high`, never
loaded rows, scan occurrence counters, or the legacy system-detail summary.
The two severity fields count distinct IDs over the same complete source and
filter scope; the existing `severity` fields still count findings. Unknown
severity remains included in `total_cves`.

Only a validated completed Current source with matching system, request epoch,
and reported generation supplies numeric header values. A newer failed attempt
does not replace an eligible earlier completed source. Loading, read failure,
missing source, invalid running identity, and older servers without distinct
severity metadata use the existing `—` and `running exposure unavailable`
treatment. A completed empty source supplies zero. Explicit historical or
built-not-deployed inspection cannot rewrite the running header.

The presentation reference is `SystemDetail.jsx`, header metric strip, at
`6e4ea52022a575550a420977baea1792e3410cef`. The repair preserves the metric's
layout, label, icons, and token styles. SC1's original header exclusion remains
historical scope, not a claim that this later narrow repair completes SC2.

### Historical and exact-scan presentation

An explicit historical selection remains inspection-only under the continuity
design, Section 7. Its Triage cell uses `read-only`, the existing `chip-unknown`
token, and the tooltip `This selected scan cannot authorize triage`. The reference
is `SystemDetail.jsx`, `CvesTab`, at
`6e4ea52022a575550a420977baea1792e3410cef`. This presentation does not change the
`legacy` API authority or make mapped-running, loading, failed, or denied states
equivalent. Authorized Current triage still depends on server-issued authority.

The exact Scanning drawer presents numeric Findings only for `completed` scans.
This includes completed clean scans and supported legacy completed scans. A
failed scan can have a terminal timestamp and default zero counters without a
completed finding set. Its drawer omits Findings and retains identity, metadata,
failure diagnostics, logs, and retry controls. Do not substitute a previous scan's
counts into that execution. System inventory can independently retain an earlier
completed source after a newer attempt fails. The presentation reference is
`ScanningView.jsx`, `ScanLogDrawer`, guarded by `cfg.found`, at the same revision.
See the [scan operations contract](../api/cve-scan-operations-api.md).

TASK-490.16 exercises these states in the existing
`integration-assurance` scenario `evidence-remediation-lifecycle`. Presentation
availability does not establish schema-1 integrity or Current mutation authority.

## Related concepts

- [Design handoff for Systems design and System Detail CVEs](systems-design-handoff-bundle.md)
- [SC1 handoff, validation, and design review](system-cves-sc1-handoff-and-validation.md)
- [Systems view architecture and consistency contract](systems-view-design-specification.md)

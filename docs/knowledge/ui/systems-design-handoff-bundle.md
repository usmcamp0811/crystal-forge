---
type: Design Specification
title: "Design handoff for Systems design and System Detail CVEs"
description: "Describes the Systems design and System Detail CVEs handoff bundle, its two README files, its file inventory, and where each file is documented; open it to navigate the SC1 design documents."
tags:
  - crystal-forge
  - web-ui
  - design-handoff
  - systems
  - cves
implementation_status: partial
generated:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-03T22:54:52-05:00
sources:
  - id: s1
    resource: "Crystal Forge repository file docs/design/CrystalForge/docs/README.md at commit 3b23d36f"
    title: "System Detail CVEs: first implementation handoff"
  - id: s2
    resource: "Crystal Forge repository file docs/design/CrystalForge/docs/crystal-forge-systems-design/README.md at commit 3b23d36f"
    title: "Systems design and System Detail CVEs handoff"
---
# Design handoff for Systems design and System Detail CVEs

This pointer concept describes the Systems design handoff bundle. Two README files introduce it, and both stay at their original paths inside the design handoff tree `docs/design/CrystalForge/`:

- [docs/README.md](../../design/CrystalForge/docs/README.md) is the archive-level note titled "System Detail CVEs: first implementation handoff".
- [crystal-forge-systems-design/README.md](../../design/CrystalForge/docs/crystal-forge-systems-design/README.md) is the bundle-level note titled "Systems design and System Detail CVEs handoff".

The design handoff tree stays in place because Nix packages, fixture seeding, and checks read it by path, and later handoffs overwrite it.

## What the two READMEs say

The archive-level README states that the handoff contains complete files, not patches. Paths under `docs/` match repository paths. The main Systems document keeps its filename and carries internal version 0.2 in that README. It names `system-cves-chunk-1.md` as the starting point. It says the original fleet CVEs, Compliance, and continuity-proposal documents are not replaced, that the newer scoped decisions are in Systems Section 22, and that the handoff changed no application code or repository state.

The bundle-level README updates that picture:

- **Versions.** The Systems document carries internal version 0.3. The SC1 contract, agent prompt, and manual guide are handoff revision 2. The SC1 contract file itself states revision 3.
- **Authority.** The bundle records the earlier SC1 handoff. The CVE/POA&M continuity design, Section 29, supersedes the retained-artifact gate for Current CVE action and cross-revision POA&M behavior. Config and rollback keep separate authority.
- **UI authority.** The owner's Claude design implementation is the UI source of truth. The Systems architecture defines data and behavior, not replacement UI. A missing design state returns to the Claude design workflow and blocks UI acceptance only.
- **Scope of SC1.** Target selection, scan selection, and trusted reconciliation of a known external activation. Unmapped Current stays unmapped. Header and count consolidation is SC2. Host and environment triage controls and cross-generation continuity are later slices.
- **Evidence.** Original application audit commit `58006084`, decision-update inspection commit `327d03b6`. The bundle contains 12 Mermaid diagrams, two screenshot crops, and a review manifest. The 36-case broad regression matrix is retained. SC1 has 15 cases including the design-authority gate.

## Bundle contents

| Retained file | Role | Pointer concept |
| --- | --- | --- |
| [systems-view-design-v0.1.md](../../design/CrystalForge/docs/crystal-forge-systems-design/systems-view-design-v0.1.md) | Systems architecture, data provenance, and consistency contract (version 0.3). | [Systems view architecture and consistency contract](systems-view-design-specification.md) |
| [system-cves-chunk-1.md](../../design/CrystalForge/docs/crystal-forge-systems-design/system-cves-chunk-1.md) | SC1 contract. | [SC1 contract](system-cves-sc1-contract.md) |
| [system-cves-chunk-1-agent-prompt.md](../../design/CrystalForge/docs/crystal-forge-systems-design/system-cves-chunk-1-agent-prompt.md) | Implementation-agent prompt for SC1. | [SC1 handoff, validation, and design review](system-cves-sc1-handoff-and-validation.md) |
| [system-cves-chunk-1-manual-validation.md](../../design/CrystalForge/docs/crystal-forge-systems-design/system-cves-chunk-1-manual-validation.md) | Manual validation guide for SC1. | [SC1 handoff, validation, and design review](system-cves-sc1-handoff-and-validation.md) |
| [system-cves-sc1-implementation.md](../../design/CrystalForge/docs/crystal-forge-systems-design/system-cves-sc1-implementation.md) | SC1 implementation and verification notes. | [SC1 handoff, validation, and design review](system-cves-sc1-handoff-and-validation.md) |
| `diagrams/` (12 `.mmd` files, `01-screen-navigation` to `12-continuity-and-readiness`) | Mermaid sources for navigation, data model, read paths, Current CVE authority, retention, and continuity. | Not migrated. The retained Systems document links them. |
| `detail-evidence.png`, `scanning-evidence.png` | Screenshot crops of the inconsistency under review. | Not migrated. They are evidence, not UI approval. |
| `review-manifest.json` | Review manifest for the bundle. | Not migrated. |

## Implementation status

> **Status:** partial. SC1 shipped in part and was then superseded in part. The status of SC2 to SC5 was not checked in this migration phase.

Evidence checked:

- `packages/default/crates/cf-server/src/queries/cves.rs` and `packages/web-ui/src/views/system_detail.rs` contain the `mapped_running` read tier.
- Migrations `0277_external_current_generation_provenance.sql` and `0278_count_archived_external_candidates.sql` add the `binding_origin` provenance for retained generations.
- Migrations `0279_cve_poam_current_evidence_continuity.sql` to `0281_cve_closed_baseline_replay_history.sql` and `view_current_cve_authority` in `packages/default/crates/cf-server/src/queries/cves.rs` implement the continuity design that supersedes the SC1 retained-artifact gate.
- The READMEs call TASK-326.2.2 "in progress". The status of that task was not checked.

## Related concepts

- [Systems view architecture and consistency contract](systems-view-design-specification.md)
- [SC1 contract](system-cves-sc1-contract.md)
- [SC1 handoff, validation, and design review](system-cves-sc1-handoff-and-validation.md)
- [Systems deployment progress, real activity, and rollback specification](systems-deployment-progress-spec.md)

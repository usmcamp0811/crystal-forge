---
type: Design Specification
title: "Compliance Implementation Roadmap (retained Backlog document)"
description: "Points to the retained Backlog document doc-12 that sequences compliance work in phases (policy bridge, MVP domain and evaluator, backend-backed UX and interop, final design parity) with readiness gates and overlap decisions."
tags:
  - crystal-forge
  - compliance
  - roadmap
  - planning
  - backlog
implementation_status: partial
generated:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-03T22:58:48-05:00
sources:
  - id: origin
    resource: "Crystal Forge repository file backlog/docs/design/doc-12%20-%20Compliance-implementation-roadmap.md at commit 3b23d36f"
    title: "Compliance implementation roadmap"
---
# Compliance Implementation Roadmap (retained Backlog document)

> **Status:** partial. The retained document is a June 2026 planning record (created 2026-06-10). Its task IDs (`TASK-307` to `TASK-319`, `TASK-334`) and milestones (`m-16`, `m-17`, `m-20`) are Backlog identifiers. The Backlog task files in this tree list several of these tasks as `Backlog`, while code for bundles, requirement mappings, waivers, and POA&M export exists. This pointer does not resolve that mismatch. It is a verification candidate.

## Retained file

[doc-12 - Compliance-implementation-roadmap.md](<../../../backlog/docs/design/doc-12 - Compliance-implementation-roadmap.md>) (90 lines). Backlog.md manages the file by ID, so it stays in place.

## What the document specifies

It clarifies how the compliance backlog fits together so that deployment-policy bridge work, first-class compliance domain work, and final UI parity work do not overlap.

- **Phase 0, policy bridge:** `TASK-307` control mapping metadata, `TASK-308` evidence capture for deployment policy evaluations, `TASK-309` waiver and exception workflow for deployment policy gates.
- **Phase 1, compliance MVP domain and evaluator:** `TASK-312` bundles and controls, `TASK-313` control-to-policy mappings, `TASK-314` layered control evaluation, `TASK-315` evidence taxonomy and provenance, `TASK-316` waiver lifecycle API, `TASK-317` evaluator and persisted rollups.
- **Phase 2, backend-backed UX and interop:** `TASK-319` UI skeleton and information architecture, `TASK-318` OHDF/HDF and OSCAL export endpoints.
- **Phase 3, final design parity:** `TASK-334` final Compliance view parity.
- **Overlap decisions:** `TASK-309` is a bridge that should be migratable into `TASK-316`; `TASK-308` should use stable IDs and taxonomy values so `TASK-315` can absorb them; `TASK-307` is a bridge, not the long-term mapping architecture.
- **Readiness gates:** MVP gate (`TASK-312` to `TASK-317` complete, outcomes persisted, evaluator deterministic and fail-closed), UX gate (truthful backend-backed flows and baseline screenshots), final parity gate (`TASK-334` with design parity proved by `checks/web-ui`).

## Implementation status and evidence

- Code for bundles, versions, assignments, framework requirements, finding waivers, and OSCAL POA&M export exists: `packages/default/crates/cf-server/migrations/0143_create_compliance_bundles.sql`, `0197_compliance_versioning.sql`, `0211_compliance_frameworks.sql`, `src/services/oscal_poam_export.rs`, and `/api/v1/finding-waivers` in `src/bin/server.rs`.
- An OHDF/HDF export route was not found in `src/bin/server.rs`.
- Task completion status was not verified from Backlog data in this phase.

## Related concepts

- [Compliance view design review](../ui/compliance-view-design.md): the later audit of the production route.
- [Compliance UI redesign spec](../ui/compliance-ui-redesign-spec.md): the specification for the final view parity.
- [CF-XCCDF interchange profile](cf-xccdf-interchange-profile.md): interchange work that followed Phase 2.

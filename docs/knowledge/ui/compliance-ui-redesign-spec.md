---
type: Design Specification
title: "Compliance UI Redesign Spec (retained Backlog document)"
description: "Points to the retained Backlog document doc-22, the visual and interaction specification for the production Compliance view (bundle table, bundle drawer, requirement coverage, systems drilldown, STIG import pause and resume) from design commit 23c88aba."
tags:
  - crystal-forge
  - compliance
  - web-ui
  - design
  - stig-import
implementation_status: partial
generated:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-03T22:58:48-05:00
sources:
  - id: origin
    resource: "Crystal Forge repository file backlog/docs/doc-22%20-%20Compliance-UI-Redesign-Spec-design-commit-23c88aba.md at commit 3b23d36f"
    title: "Compliance UI Redesign Spec design commit 23c88aba"
---
# Compliance UI Redesign Spec (retained Backlog document)

> **Status:** partial. The retained document (created 2026-08-15, updated 2026-09-12) is an implementation specification. Several of its features exist in code (see evidence below), and it carries a supersession note: the section 5.2 instruction to keep or relocate the bundle-detail `Assign bundle` panel is superseded by TASK-440 residual cleanup. Assignment creation, update, and removal belong to the Environment editor, and bundle detail keeps read-only Systems assignment metadata and existing-assignment maintenance only. Visual parity of the whole specification has not been verified.

## Retained file

[doc-22 - Compliance-UI-Redesign-Spec-design-commit-23c88aba.md](<../../../backlog/docs/doc-22 - Compliance-UI-Redesign-Spec-design-commit-23c88aba.md>) (796 lines). Backlog.md manages the file by ID, so it stays in place.

## What the document specifies

It is the authoritative visual and interaction specification for rebuilding the Dioxus Compliance view to match design commit `23c88aba`. Source precedence is: production data and behavior semantics declared in the spec (and the TASK-418 model) first, then preserved TASK-418 functionality, then `23c88aba` for visual geometry and interactions.

Section list:

1. Prerequisite: MR !315 (TASK-418, the normalized framework, requirement, and mapping model) must be merged first
2. Design mock to Crystal Forge domain mapping (no N+1 fetching; display `{requirement_count} requirements · {policy_count} policies`, never the deprecated `control_count` alias)
3. Page layout (the left bundle rail is removed; bundle detail renders in a drawer)
4. Bundle list table (filter header, empty state, table, score colour function, publication-state chip)
5. Bundle detail drawer (header, overview body, a single source of truth for the selected bundle version, requirement-coverage summary, coverage view, policy drawer drill-in, systems drilldown)
6. CSS additions (verify in dark and light themes)
7. Required backend changes: (a) per-bundle `aggregate_score` and `applicable_system_count` on the bundle list, reusing the existing rollup functions; (b) `policy_id` on coverage mappings
8. STIG import pause and resume (modal behavior, paused-import callout, TASK-418 preservation invariant for the import state machine, versioned size-guarded draft persistence that never stores raw uploaded bytes)
9. Reviewer verification endpoints (design example, local golden fixture, reference screenshots, state-by-state comparison table, backing API endpoints)
10. Required implementation sequence (Phases 0 to 6, each ending at a STOP point)
11. Verification
12. Out of scope (the normalized mapping model, the Policies view, waiver workflow, evidence taxonomy, new evaluation logic), with the 2026-09-12 supersession note

## Implementation status and evidence

- Section 7(a): `ComplianceBundleSummary` in `packages/default/crates/cf-server/src/api/models.rs` has `applicable_system_count` and `aggregate_score`; the web UI mirrors them in `packages/web-ui/src/api/models.rs` and uses them in `packages/web-ui/src/components/compliance/mod.rs`.
- Section 8: `packages/web-ui/src/views/compliance.rs` defines `STIG_IMPORT_DRAFT_KEY = "cf-stig-import-draft"`, `MAX_STIG_IMPORT_DRAFT_BYTES = 2 MiB`, and a paused-import callout.
- Section 1 routes: `/api/v1/compliance/bundle-versions/:bv_id/requirement-coverage` exists in `src/bin/server.rs`.
- Not checked: Sections 3 to 6 visual geometry, Section 7(b) `policy_id`, the Section 9 screenshots, and the Section 11 check steps (`29` to `29e`).

## Related concepts

- [Compliance view design review](compliance-view-design.md): a later audit of the same route.
- [TASK-433 design parity review](../historical/task-433-design-parity-review.md): the later design parity review.
- [Compliance implementation roadmap](../compliance/compliance-implementation-roadmap.md): the roadmap that places view parity last.
- [CF-XCCDF interchange operator guide](../compliance/cf-xccdf-interchange-operator-guide.md): the XCCDF preview and import API that the pause and resume flow uses.

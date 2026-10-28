---
type: Historical Reference
title: "All-views visual drift audit specification"
description: "Summarizes Backlog spec doc-19, the one-time guide for auditing every web UI view against the updated design example and classifying drift as small or large."
tags:
  - crystal-forge
  - web-ui
  - design-parity
  - audit
implementation_status: historical
status: deprecated
generated:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-03T22:54:52-05:00
sources:
  - id: s1
    resource: "Crystal Forge repository file backlog/docs/specs/doc-19%20-%20Spec-All-views-visual-drift-audit-against-updated-design-example.md at commit 3b23d36f"
    title: "Spec: All-views visual drift audit against updated design example"
---
# All-views visual drift audit specification

This pointer concept describes Backlog document `doc-19`, [Spec: All-views visual drift audit against updated design example](../../../backlog/docs/specs/) (file `doc-19 - Spec-All-views-visual-drift-audit-against-updated-design-example.md`). Backlog.md manages the document by ID, so it stays at its path. The retained file is the authoritative text. It is the guide for a one-time audit task. It states that the design example in `docs/design/CrystalForge/` was updated on 2026-07-07 and that earlier parity work targeted an older snapshot.

## What it specifies

| Section | Content |
| --- | --- |
| 0 Focus rule | Classify each discrepancy. A SMALL drift is presentational only (text, chips, icons, spacing, layout geometry, minor static elements, hover and selected states), confined to existing components, and needs no new API data, route, interaction flow, or backend change. A SMALL drift is fixed inline. A LARGE gap is filed as a follow-up Backlog task and not implemented. When unsure, classify it as LARGE. |
| 1 How to compare | Render the design example (`serve.sh`) and the implementation (`run-ui-dev` or the web-ui check screenshots). The design-parity harness under `checks/web-ui/design-parity/` is the preferred objective mechanism. |
| 2 Audit checklist | A 17-row table of views, design source files, and implementation files: Dashboard, Systems list, System detail, Flakes, Environments, Builds, Evaluations, Scanning, CVEs, Policies, Compliance, Builders, Caches, Admin, Shell chrome, Add and Edit system modals, and Deploy gate. |
| 3 Known large gaps | The missing Profile view, and the open tasks that must not be duplicated. |
| 4 Verification | Formatting, lint, tests, the web-ui check, extended design-parity manifest coverage, and before and after screenshots in the merge request. |
| 5 Out of scope | LARGE items, backend changes, view refactors beyond presentational fixes, and mobile or responsive redesign. |

## Implementation status

> **Status:** historical. The document guides a finished audit. The state it describes no longer holds.

Evidence checked: the "Profile view is entirely missing" gap is stale. `packages/web-ui/src/views/profile.rs` exists and `packages/web-ui/src/routes.rs` routes `ProfileView`. The design-parity harness exists at `checks/web-ui/design-parity/` (`manifest.json`, `generate-design-targets.js`, `compare-design-parity.js`). Whether the audit task finished and filed its follow-ups was not checked.

## Related concepts

- [UI parity program planning documents](../historical/ui-parity-program-documents.md)
- [Flakes sync-error surfaces and sidebar alert badge specification](flakes-sync-errors-and-sidebar-badges-spec.md)
- [Crystal Forge UI/UX Design System](design-system-overview.md)

---
type: Design Specification
title: "Flakes sync-error surfaces and sidebar alert badge specification"
description: "Summarizes Backlog spec doc-18 for flake sync status recording, the navigation badge endpoint, the sidebar badge and attention flash system, and flake sync-error components."
tags:
  - crystal-forge
  - web-ui
  - flakes
  - sidebar
  - alerts
implementation_status: partial
generated:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-03T22:54:52-05:00
sources:
  - id: s1
    resource: "Crystal Forge repository file backlog/docs/doc-18%20-%20Spec-Flakes-sync-error-surfaces-and-sidebar-alert-badge-system.md at commit 3b23d36f"
    title: "Spec: Flakes sync-error surfaces and sidebar alert badge system"
---
# Flakes sync-error surfaces and sidebar alert badge specification

This pointer concept describes Backlog document `doc-18`, [Spec: Flakes sync-error surfaces and sidebar alert badge system](../../../backlog/docs/specs/) (file `doc-18 - Spec-Flakes-sync-error-surfaces-and-sidebar-alert-badge-system.md`). Backlog.md manages the document by ID, so it stays at its path. The retained file is the authoritative text. It is the implementation guide for the flake sync status and sidebar badge task.

## What it specifies

| Section | Content |
| --- | --- |
| 0 Ground truth | Design references in `FlakesView.jsx`, `Shell.jsx` (the sidebar badge system and attention flash), and `styles.css`, plus the implementation files to read first. |
| 1 Migration | New columns `sync_status` (`unknown`, `synced`, `syncing`, `error`), `last_sync_at`, and `last_sync_error` on `flakes`, with a check constraint. |
| 2 Record sync outcomes | One wrapper, `sync_flake_recorded`, sets `syncing`, then `synced` or `error`, truncates error text to 4,000 characters, and replaces all four call sites. Status writes are best-effort. The flakes API exposes the three new fields. |
| 3 Badge aggregate endpoint | `GET /api/v1/navigation/badges` returns a `NavigationBadges` DTO with attention and total counts per section. The query rules say to reuse existing health semantics and the existing CVE stats query. |
| 4 Sidebar badges and attention flash | A DTO and client call, an `alerts` module with a pure acknowledgment and flash core, `SidebarNav` integration polled every 30 seconds with tooltips, and per-view `attention-flash` wiring. Builds and Evaluations acknowledge only when the tab that holds failures opens. |
| 5 Flakes sync-error surfaces | `FlakeSyncChip`, `FlakeSyncErrorBanner`, a card error callout, a table status column, and a subtitle that counts real `synced` flakes. All new components go in `packages/web-ui/src/components/flake/`. |
| 6 to 8 | Fixtures with one errored flake, web-ui check steps with mandatory screenshots, minimum tests, and verification commands. |
| 9 Out of scope | The visual drift audit, the notification bell and topbar notifications, flake environment span data, and auto-sync interval persistence. |

## Implementation status

> **Status:** partial. The feature shipped, and later work changed the badge semantics that this guide describes.

Evidence checked:

- Migration `0158_flake_sync_status.sql` exists. The guide assumed number `0157`. Migration `0170_flake_sync_attempt_id.sql` extends the sync status later.
- `sync_flake_recorded` exists in `packages/default/crates/cf-server/src/queries/flakes.rs`.
- `FlakeSyncChip` and `FlakeSyncErrorBanner` exist in `packages/web-ui/src/components/flake/`.
- `GET /api/v1/navigation/badges` exists. The DTO and counting rules differ from the guide. The server module `packages/default/crates/cf-server/src/queries/navigation.rs` now counts eligible undismissed canonical occurrences opened in the last 24 hours, and the DTO carries `observed_at`, fingerprint, and occurrence-ID fields. The guide describes live counts such as `flakes_errored` and `cves_critical`. Counts such as `builds_failed_new` and `cves_critical_new` replaced some fields.
- `packages/web-ui/src/alerts/mod.rs` provides acknowledgment and attention flash, now with server-side persistence per user (the module attributes this to a TASK-385 follow-up). The guide describes acknowledgment as local to the page load.
- The paths in the retained guide (`packages/default/src/...`) are stale. The crates live under `packages/default/crates/`.

## Related concepts

- [Sidebar badges versus the notification bell](alerts-and-notifications-decision.md) records the design decision that the badge system implements.
- [Systems deployment progress, real activity, and rollback specification](systems-deployment-progress-spec.md)
- [All-views visual drift audit specification](all-views-visual-drift-audit-spec.md)

---
type: Decision
title: "Sidebar badges versus the notification bell"
description: "Records the design decision that separates live sidebar badges from the chronological notification bell, with a rule for choosing between them; open it before adding an attention signal."
tags:
  - crystal-forge
  - web-ui
  - alerts
  - notifications
  - sidebar
implementation_status: partial
generated:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-03T22:54:52-05:00
sources:
  - id: s1
    resource: "Crystal Forge repository file docs/design/CrystalForge/docs/alerts-and-notifications.md at commit 3b23d36f"
    title: "Sidebar badges vs. the notification bell"
---
# Sidebar badges versus the notification bell

This pointer concept records a design decision. The authoritative text stays in [alerts-and-notifications.md](../../design/CrystalForge/docs/alerts-and-notifications.md) inside the design handoff tree `docs/design/CrystalForge/`. That tree stays at its original path because Nix packages, fixture seeding, and checks read it by path.

## What the retained document decides

The document separates two attention surfaces that look alike:

- **Sidebar badges** answer "is this section OK right now?". Each badge is a live rollup of current, unresolved state for one navigation section. The document lists the sources per section: Systems (critical or offline health, pending deploy approvals, unresolved attestation attention items), Flakes (flakes failing to sync), Environments (environments with at least one critical or offline system), Evaluations and Builds (failures in the last 24 hours), and CVEs (open critical CVEs).
- **The notification bell** answers "what happened, and did I deal with it?". It is a chronological event log. Each entry is timestamped, keeps its own read state, and persists until the operator reads or dismisses it. The document lists these sources: deploys newly awaiting approval, unauthorized, unknown, or invalid-identity attestation findings, build failures, new critical CVEs, lost heartbeats, and completed evaluations.

The rule of thumb for a new signal: a standing condition that would still be true if nobody looked at it for a week gets a sidebar badge. A discrete event that is worth logging after it resolves gets a bell entry. Some conditions use both, for example a pending deploy approval.

## Sections in the retained document

| Section | Content |
| --- | --- |
| Sidebar badges | Per-section sources, the disappearance rules, the "gauge, not an inbox" rule, and tooltip requirement. |
| Notification bell | Event-log semantics, current sources, additive entries, and click-through routing. |
| Rule of thumb when adding a new signal | The standing-condition versus discrete-event test. |

## Implementation status

> **Status:** partial. Both surfaces exist. The badge semantics in the code differ from the retained decision text. The retained file was not edited.

Evidence checked:

- `GET /api/v1/navigation/badges` is registered in `packages/default/crates/cf-server/src/bin/server.rs`. The counts come from `packages/default/crates/cf-server/src/queries/navigation.rs`. That module documents that counts are now "eligible undismissed canonical occurrences" opened within the last 24 hours, with a per-user dismissal contract.
- `packages/web-ui/src/alerts/mod.rs` keeps badge acknowledgment per authenticated user and persists it on the server. It also holds `dismissed_items` for individual attention rows.
- The retained decision says a badge has no history and no per-item dismissal and disappears when the operator visits the section. The code uses persisted per-user acknowledgment and dismissal of canonical occurrences. A verification pass must decide whether to update the decision text or the code.
- The notification feed is implemented in `packages/web-ui/src/components/layout/topbar.rs`. The server stores user notifications (migration `0226_user_notifications_sessions.sql` and later notification migrations). The list of bell sources in the retained document was not compared with the server event types.

## Related concepts

- [Flakes sync-error surfaces and sidebar alert badge specification](flakes-sync-errors-and-sidebar-badges-spec.md) records the implementation guide for the sidebar badge system.
- [Crystal Forge UI/UX Design System](design-system-overview.md)
- [Design handoff for Systems design and System Detail CVEs](systems-design-handoff-bundle.md)

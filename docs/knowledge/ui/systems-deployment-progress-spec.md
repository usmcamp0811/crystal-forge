---
type: Design Specification
title: "Systems deployment progress, real activity, and rollback specification"
description: "Summarizes Backlog spec doc-17 for live deployment progress stages, the deployment-started agent report, real recent activity, and the rollback production guard; open it to find the retained implementation guide."
tags:
  - crystal-forge
  - web-ui
  - systems
  - deployment
  - rollback
implementation_status: implemented
generated:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-03T22:54:52-05:00
sources:
  - id: s1
    resource: "Crystal Forge repository file backlog/docs/specs/doc-17%20-%20Spec-Systems-view-live-deployment-progress-real-recent-activity-working-rollback.md at commit 3b23d36f"
    title: "Spec: Systems view live deployment progress, real recent activity, working rollback"
---
# Systems deployment progress, real activity, and rollback specification

This pointer concept describes Backlog document `doc-17`, [Spec: Systems view live deployment progress, real recent activity, working rollback](../../../backlog/docs/specs/) (file `doc-17 - Spec-Systems-view-live-deployment-progress-real-recent-activity-working-rollback.md`). Backlog.md manages the document by ID, so it stays at its path. The retained file is the authoritative text. It is the implementation guide for a companion backlog task (the Systems deployment, activity, and rollback work, referred to there as TASK-384).

## What it specifies

The guide tells an implementer to follow nine sections top to bottom and to copy named patterns. Its decisions:

| Section | Content |
| --- | --- |
| 0 Ground truth | Design references in `docs/design/CrystalForge/components/SystemDetail.jsx`, `Systems.jsx`, `styles.css`, and the current implementation files to read first. |
| 1 Stage contract | A deployment flows through observable stages derived server-side from `pending_system_deployments` columns and never stored as a string: `queued`, `picked_up`, `applying`, `activated`, `failed`. Superseded and expired rows are not shown as active. The UI must tolerate a jump from `picked_up` to `activated` because old agents never report "applying". |
| 2 Migration | A new migration adds `delivered_at` and `applying_at` to `pending_system_deployments`. Existing migrations are never edited. |
| 3 Server | A `source` parameter distinguishes `manual_deploy`, `manual_rollback_commit`, and `manual_rollback_generation`. The heartbeat handler marks `delivered_at`. A new agent endpoint `POST /agent/deployment-started` sets `applying_at` and records a `cf_deployment_started` event idempotently. The history endpoint maps that event. A new `GET /api/v1/systems/:id/deployment-status` returns a `SystemDeploymentProgress` row, or 204 when idle. |
| 4 Agent | The agent reports deployment-started before `switch-to-configuration`, fire-and-forget, with a 5-second timeout. A failure never blocks a deployment. |
| 5 Web UI | DTO and client, a reusable `PendingDeployBanner`, System Detail integration with 4-second polling only while a banner-worthy state exists, a real Recent activity feed in place of synthetic data, a rollback modal with a production type-to-confirm guard, and the Systems list slide-out panel. |
| 6 Fixtures and checks | Fixture data for one system and web-ui check steps with mandatory screenshots. |
| 7 to 8 Tests and verification | Minimum unit and database tests and the Nix verification commands. |
| 9 Out of scope | SSE or websocket streaming, SSH modal, tags persistence, Deploy tab parity, dashboard widgets, scheduler policy changes, and edits to existing migrations. |

## Implementation status

> **Status:** implemented, with details not rechecked. The core pieces exist in the code.

Evidence checked:

- Migration `packages/default/crates/cf-server/migrations/0156_deployment_progress_tracking.sql` exists.
- The agent reports to `/agent/deployment-started` in `packages/default/crates/cf-agent/src/deployment/agent.rs`. The route is registered in `packages/default/crates/cf-server/src/bin/server.rs` with a handler in `packages/default/crates/cf-server/src/handlers/agent/deployment_started.rs`.
- `/api/v1/systems/:id/deployment-status` is registered in `packages/default/crates/cf-server/src/bin/server.rs`. `cf_deployment_started` is handled in `packages/default/crates/cf-server/src/handlers/api/systems.rs`.
- `PendingDeployBanner` exists in `packages/web-ui/src/components/system/pending_deploy_banner.rs`. `get_system_deployment_progress` exists in `packages/web-ui/src/api/client.rs`. The rollback modal in `packages/web-ui/src/views/system_detail.rs` contains the policy-bypass warning and a "Roll back to gen" label.
- The paths in the retained guide (`packages/default/src/...`) predate the move to `packages/default/crates/cf-server/` and `packages/default/crates/cf-agent/`. The paths are stale. The migration directory is `packages/default/crates/cf-server/migrations/`.
- The real Recent activity feed, the 4-second polling rule, the production type-to-confirm guard, and the fixture and check steps were not compared with the code.

## Related concepts

- [Sidebar badges versus the notification bell](alerts-and-notifications-decision.md)
- [Systems view architecture and consistency contract](systems-view-design-specification.md) lists this guide as an existing specification and records its limitations.
- [Flakes sync-error surfaces and sidebar alert badge specification](flakes-sync-errors-and-sidebar-badges-spec.md)

---
id: doc-17
title: >-
  Spec: Systems view live deployment progress, real recent activity, working
  rollback
type: specification
created_date: '2026-07-08 02:48'
tags:
  - systems
  - deployment
  - design-parity
  - web-ui
  - agent
---
# Spec: Systems view — live deployment progress, real recent activity, working rollback

This is the implementation guide for the companion backlog task. Follow it top to bottom. Every step names the exact file and the pattern to copy. Do not improvise new architecture.

## 0. Ground truth — read these files FIRST

Design reference (the target look and behavior):
- `docs/design/CrystalForge/components/SystemDetail.jsx` — `PendingDeployBanner` (lines 1-48), `DEPLOY_STAGES` (line 52), header Rollback button (line 104), heartbeat metric override during deploy (lines 118-126), banner placement (lines 153-164), `RollbackModal` with production type-to-confirm (lines 210-285), `buildActivityFeed` + Recent activity card (lines 349-501).
- `docs/design/CrystalForge/components/Systems.jsx` — slide-out `SystemPanel` with `PendingDeployBanner` (lines 210-221) and Recent activity section (lines 273-292).
- `docs/design/CrystalForge/styles.css` — `.deploy-pending`, `.deploy-step*`, `.deploy-pending.rollback` styles (around lines 700-760), `.tl-item-live`, `.tl-dot-pulse`.

Current implementation:
- `packages/web-ui/src/views/system_detail.rs` — `OverviewTab` (line ~1755), fake Recent activity (line ~1886), `GenerationRollbackModal` (line ~1470), header Rollback button (line ~690).
- `packages/web-ui/src/views/systems_list.rs` — `SystemPreviewPanel` (line ~1050), its Recent activity section (line ~1304).
- `packages/web-ui/src/components/modals/remove_system_dialog.rs` — EXISTING production type-to-confirm pattern (line ~30). Reuse this exact pattern for rollback.
- `packages/default/src/handlers/api/systems.rs` — `deploy_system` (~1471), `rollback_system` (~940), `rollback_system_generation` (~1009), `get_system_history` (~1914), `event_history_kind` (~1894).
- `packages/default/src/handlers/agent/heartbeat.rs` — serves `desired_target` in heartbeat response (~line 314 via `get_agent_desired_target_by_hostname`).
- `packages/default/src/handlers/agent/state.rs` — agent auth pattern (`authenticate_agent_request_with_lookup`) and transactional event recording (`record_report_events_tx`).
- `packages/default/src/queries/system_events.rs` — `pending_system_deployments` queries: `set_pending_deployment_target_tx`, `match_pending_deployment` SQL, event insert with dedupe.
- `packages/default/src/deployment/agent.rs` — agent-side `AgentDeploymentManager::execute_deployment` (runs `switch-to-configuration`).
- `packages/default/migrations/0155_system_events_timeline.sql` — `pending_system_deployments` + `system_events` schema. NOTE: `cf_deployment_started` is ALREADY allowed by the `system_events_event_type_check` constraint but nothing emits it yet.

## 1. The deployment stage contract

A deployment/rollback flows through 4 observable stages. The stage is DERIVED server-side from `pending_system_deployments` columns — never stored as a string column:

| Stage | Condition |
|---|---|
| `queued` | `status = 'pending'` AND `delivered_at IS NULL` |
| `picked_up` | `status = 'pending'` AND `delivered_at IS NOT NULL` AND `applying_at IS NULL` |
| `applying` | `status = 'pending'` AND `applying_at IS NOT NULL` |
| `activated` | `status = 'succeeded'` |
| `failed` | `status = 'failed'` |

Rows with `status IN ('superseded','expired')` are not shown as active. Old agents never report "applying" — the UI MUST tolerate a jump from `picked_up` straight to `activated`.

## 2. Database migration (NEW file — NEVER edit existing migrations)

Create `packages/default/migrations/0156_deployment_progress_tracking.sql`:

```sql
ALTER TABLE pending_system_deployments
    ADD COLUMN IF NOT EXISTS delivered_at timestamptz,
    ADD COLUMN IF NOT EXISTS applying_at timestamptz;
```

That is the entire migration. `source` and `metadata` columns already exist for labeling deploy vs rollback.

## 3. Backend server changes (`packages/default`)

### 3.1 Distinguish deploy vs rollback in `source`
The manual endpoints funnel through `update_desired_target` (`packages/default/src/queries/deployment.rs`), which calls `set_pending_deployment_target_tx(..., "auto_desired_target")`. Thread a `source: &str` parameter through so:
- `deploy_system` → `"manual_deploy"`
- `rollback_system` (commit) → `"manual_rollback_commit"`
- `rollback_system_generation` → `"manual_rollback_generation"`
- auto-deployment scheduler (`packages/default/src/deployment/mod.rs` line ~373) keeps `"auto_desired_target"`.
Keep the default behavior for all existing callers you do not intentionally change.

### 3.2 Mark `delivered_at` in the heartbeat handler
In `packages/default/src/handlers/agent/heartbeat.rs`, after `get_agent_desired_target_by_hostname` returns `Some(target)` that is served to the agent: call a new query fn `mark_pending_deployment_delivered(pool, system_id, &target)` that runs:

```sql
UPDATE pending_system_deployments
SET delivered_at = now()
WHERE system_id = $1 AND target_store_path = $2
  AND status = 'pending' AND delivered_at IS NULL
```

Put the fn in `packages/default/src/queries/system_events.rs` next to the other pending-deployment queries. It must be a no-op when no matching row exists. Do not fail the heartbeat if this update errors — log a warning.

### 3.3 New agent endpoint: deployment started
Add `POST /agent/deployment-started` in `packages/default/src/bin/server.rs` route table, handler in `packages/default/src/handlers/agent/` (new file `deployment_started.rs`, registered in the agent handlers module). Copy the authentication + hostname-lookup pattern from `handlers/agent/state.rs` exactly.

Payload (serde, all new fields `#[serde(default)]`):
```json
{ "hostname": "...", "target_store_path": "/nix/store/..." }
```

Handler behavior (single transaction):
1. Authenticate the agent (same as `/agent/state`).
2. Find the matching pending row (`status='pending'`, same `system_id` + `target_store_path`). If none: return 200 with a body noting no pending deployment (do NOT error — version skew is normal).
3. Set `applying_at = now()` where it is NULL.
4. Insert a `system_events` row with `event_type = 'cf_deployment_started'`, `dedupe_key = pending row id`, `deployment_id`/`desired_target_id` = pending row id, `source = 'agent_report'`, `new_store_path = target_store_path`, `occurred_at = now()`. Use the existing dedupe-safe insert helper in `queries/system_events.rs` so retries are idempotent.

### 3.4 Map `cf_deployment_started` in the history endpoint
In `packages/default/src/handlers/api/systems.rs`, extend `event_history_kind` / `event_history_title` (~line 1894) so `cf_deployment_started` maps to kind `cf_deployment`, actor `crystal-forge`, outcome `started`, title `Deployment started`. Extend the existing unit tests beside them (~line 2215+).

### 3.5 New API endpoint: deployment status
Add `GET /api/v1/systems/:id/deployment-status` (route in `bin/server.rs`, handler in `handlers/api/systems.rs`, same auth guard as `get_system_history`). Response DTO in `packages/default/src/api/models.rs`:

```rust
pub struct SystemDeploymentProgress {
    pub id: uuid::Uuid,
    pub stage: String,            // queued|picked_up|applying|activated|failed
    pub kind: String,             // "deploy" | "rollback" (derived from source: manual_rollback_* => rollback, else deploy)
    pub target_store_path: String,
    pub target_commit: Option<String>,     // resolved via derivations/commits join when available
    pub target_generation: Option<i64>,    // parsed from metadata if recorded, else None
    pub source: String,
    pub issued_at: chrono::DateTime<chrono::Utc>,
    pub delivered_at: Option<chrono::DateTime<chrono::Utc>>,
    pub applying_at: Option<chrono::DateTime<chrono::Utc>>,
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
}
```

Selection rule (one query, `sqlx::query_as`): return the newest `status='pending'` unexpired row; if none, return the newest row with `status IN ('succeeded','failed')` AND `completed_at > now() - interval '2 minutes'` (lets the UI show the green "Deployment complete / Activated" banner before it disappears); otherwise the endpoint returns `204 No Content`. Expired/superseded rows are never returned.

Write unit tests for the stage-derivation function (pure fn taking the row → stage string) covering all 5 stages + expiry.

## 4. Agent changes (`packages/default/src/deployment/agent.rs` + `bin/agent.rs`)

In `AgentDeploymentManager::execute_deployment`, immediately BEFORE launching `switch-to-configuration`: POST to `{server}/agent/deployment-started` with the hostname and target store path. Reuse the exact request-building/signing helper the agent already uses for `/agent/state` (see `bin/agent.rs` ~line 201). Rules:
- Fire-and-forget: on ANY error (404 from old server, timeout, network), log at `debug!` and CONTINUE the deployment. The report must never block or fail a deployment.
- Timeout the request at 5 seconds.

## 5. Web UI changes (`packages/web-ui`)

### 5.1 DTO + client
- `packages/web-ui/src/api/models.rs`: mirror `SystemDeploymentProgress` (all `Option` fields `#[serde(default)]`).
- `packages/web-ui/src/api/client.rs`: `pub async fn get_system_deployment_progress(system_id: Uuid) -> Result<Option<SystemDeploymentProgress>, ...>` — treat HTTP 204 as `Ok(None)`. Copy the fetch pattern from `request_system_sync` (~line 556).

### 5.2 `PendingDeployBanner` component (NEW, reusable)
New file `packages/web-ui/src/components/system/pending_deploy_banner.rs`, registered in `components/system/mod.rs`. Props: `progress: SystemDeploymentProgress`, `hostname: String`, `heartbeat_interval_secs: i64`, `on_dismiss: EventHandler<()>`, `on_view_logs: EventHandler<()>`.

Render EXACTLY per `SystemDetail.jsx` lines 19-47 using the same CSS classes: root `deploy-pending` (+` done` when activated, +` rollback` when kind=rollback), title "Deployment in progress"/"Rollback in progress" → "Deployment complete"/"Rollback complete", mono commit/target chip, per-stage sub text (queued shows "Waiting for {hostname} agent to check in (heartbeat every {n}s)"), 4-step tracker `deploy-steps`/`deploy-step`/`deploy-step-dot`/`deploy-step-pulse`/`deploy-step-bar` with past=check, current=pulse. `failed` stage: reuse the banner with a red/danger treatment and sub text from the design's failure language; dismiss button always available when done or failed. Verify the `.deploy-pending*` CSS classes exist in the web-ui stylesheet (`packages/web-ui/assets/` / tailwind input css); port them from `docs/design/CrystalForge/styles.css` if missing.

### 5.3 System Detail integration (`views/system_detail.rs`)
- Add a `use_resource` that calls `get_system_deployment_progress`, re-polled every 4 seconds ONLY while a banner-worthy state exists (pending stage or completed <2min). Use the same interval/polling pattern already used elsewhere in the view if present; otherwise a `use_future` loop with `gloo_timers`/async sleep matching existing web-ui conventions.
- Render the banner between the metric strip and the tabs (same slot as design line 153).
- Heartbeat metric override while a pending stage exists (design lines 118-126): spinner + "awaiting agent" / "picked up" / "applying" / green check "activated" instead of the normal `HeartbeatSpinner`.
- "Logs" button on banner switches to the logs tab; dismiss hides the banner locally (signal) until a NEW progress id appears.
- After a successful Deploy or Rollback request, immediately trigger one refetch so the banner appears without waiting for the next poll.

### 5.4 Real Recent activity (Overview tab)
Replace the synthetic `recent_activity` vec (line ~1886) entirely. Feed = REAL events only, from the same history data the History tab uses (`/api/v1/systems/:id/history` — reuse the already-fetched resource if it is in scope; otherwise fetch it in `OverviewTab`). Map each event to (icon, color, title, sub, timestamp) following the design mapping in `buildActivityFeed` (SystemDetail.jsx lines 372-385): deploy success purple `#a78bfa`, deploy started blue `#60a5fa`, failed red `#f87171`, restart blue `#60a5fa`, local rebuild reconciled blue / out-of-band amber `#fbbf24`. Cap at 9 items. Replace the "last 24h" label with a "View all" button that switches to the History tab. Empty state: keep the card with a muted "No recorded events yet" line.

### 5.5 Rollback design parity + production guard
In `GenerationRollbackModal` (line ~1470):
- Keep the existing generation candidate list (real data from `/api/v1/systems/:id/generations`).
- Add the warning callout wording from design line 250 ("Rolling back bypasses the current deployment policy...").
- Add production type-to-confirm: reuse the EXACT pattern from `components/modals/remove_system_dialog.rs` (~line 30) — when the system's environment `is_production` (fall back to case-insensitive name match `prod`/`production` when the flag is absent), require typing the hostname before the confirm button enables. Confirm button label: "Roll back to gen #N".
- On confirm success: close modal, switch to Overview tab, trigger a progress refetch so the amber rollback banner shows.
- The header Rollback button (line ~690) must open this modal (it already does — verify, do not regress). This supersedes TASK-280.

### 5.6 Systems list slide-out panel (`views/systems_list.rs`, `SystemPreviewPanel` ~line 1050)
- When the panel opens, fetch `get_system_deployment_progress` for that system (+ poll every 4s while pending, stop on close). If active: render `PendingDeployBanner` as the first panel section (design Systems.jsx lines 210-221); "Logs" routes to the full detail view.
- Replace the panel's Recent activity content (~line 1304) with real events: fetch the system history on panel open, show the newest 6 with the same mapping as 5.4.

## 6. Fixtures + web-ui check (screenshots are MANDATORY)

- `docs/design/CrystalForge/fixtures/crystal-forge.fixtures.json`: add, for exactly one existing fixture system, (a) a pending deployment object (target store path, issued/delivered/applying timestamps, source `manual_deploy`) and (b) 5+ system_events rows spanning: cf_deployment_succeeded, cf_deployment_started, cf_deployment_failed, system_reboot, local_rebuild_detected.
- `packages/default/src/fixtures/seed.rs`: seed those into `pending_system_deployments` and `system_events` (respect the dedupe constraint). Timestamps must be relative-to-now on seed so the banner is always "in flight" during checks.
- `checks/web-ui/coverage-manifest.json`: add steps that (1) open that system's detail page and assert the text "Deployment in progress" and the stage label "Applying" are visible, capture screenshot; (2) assert the Recent activity card contains at least 3 real event rows (e.g. text "Deployed"/"Deployment started"); (3) open the Rollback modal, assert the policy-bypass warning text renders and (for a production fixture system) that the confirm button is disabled until the hostname is typed, capture screenshot; (4) open the systems list panel for the deploying system and assert the banner renders there, capture screenshot.

## 7. Tests summary (minimum)

- Unit (server): stage derivation (all 5 stages + expired), `event_history_kind("cf_deployment_started")`, source→kind mapping (manual_rollback_* → rollback).
- Unit (web-ui, native target): event→activity-row mapping fn (write it as a pure helper so it is testable without rendering), production-guard enablement logic.
- DB tests (`#[ignore = "requires live database connection"]`, existing pattern in `handlers/api/systems.rs` tests): `mark_pending_deployment_delivered` only touches pending+NULL rows; deployment-started endpoint sets `applying_at` and inserts exactly one `cf_deployment_started` event on double-post; deployment-status endpoint returns the right row/stage and 204 when idle.
- SQLX: migration + new queries REQUIRE `cargo sqlx prepare` from the devshell (`db-only up` first). Never point at a shared DB.

## 8. Verification commands (from repo devshell, `nix develop`)

```
cd packages/default && cargo fmt --all -- --check && cargo clippy --all-targets -- -D warnings && cargo test
cd packages/web-ui  && cargo fmt --all -- --check && cargo clippy --all-targets -- -D warnings && cargo test
# sqlx (devshell): db-only up ; cargo sqlx prepare  (or sqlx-prepare helper)
nix build .#packages.x86_64-linux.server --no-link
nix build .#packages.x86_64-linux.web-ui --no-link
nix build .#checks.x86_64-linux.web-ui --no-link     # must produce the new screenshots
nix flake check --keep-going                          # required: migration + agent + server surface
```

MR MUST attach the web-ui check screenshots showing the banner, the real activity feed, and the rollback modal (GitLab uploads, not committed files).

## 9. Explicitly OUT of scope

- SSE/websocket streaming (polling only).
- SSH modal, tags persistence (TASK-353.1), Deploy tab full parity, dashboard widgets.
- Changing `auto_latest` policy semantics or the deployment scheduler beyond the `source` parameter.
- Editing ANY existing migration file.
- Systems list cards/table visual parity (TASK-330 / TASK-353 own that; coordinate, do not collide — this task only touches the panel's banner + activity sections).

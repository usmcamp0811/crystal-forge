---
id: TASK-378
title: Harden Agent Observability and Configurable Heartbeat Reliability
status: In Progress
assignee:
  - '@ai-agent'
created_date: '2026-07-03 16:39'
updated_date: '2026-07-05 23:23'
labels:
  - agents
  - heartbeat
  - observability
  - reliability
  - server
  - api
  - web-ui
  - database
dependencies:
  - TASK-279
  - TASK-353
references:
  - packages/default/src/bin/agent.rs
  - packages/default/src/handlers/agent/heartbeat.rs
  - packages/default/src/deployment/agent.rs
  - packages/default/src/config/server.rs
  - packages/default/src/queries/systems.rs
  - packages/default/src/api/models.rs
  - packages/default/src/services/systems.rs
  - packages/default/src/models/system_states.rs
  - packages/web-ui/src/api/models.rs
  - packages/web-ui/src/components/system/edit_system_modal.rs
  - packages/web-ui/src/components/system/system_card_v2.rs
  - packages/web-ui/src/views/system_detail.rs
  - packages/web-ui/src/views/systems_list.rs
  - checks/web-ui/tests/integration-test.js
priority: high
ordinal: 322000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

### Issue 1: Agents appear offline in UI despite service running
Agent systemd service runs with no errors, yet UI shows the system as "offline". Heartbeat POSTs fail silently (network, auth, deserialization errors) or the agent fails to heartbeat despite the service being alive.

### Issue 2: False "Detected change to /run/current-system" spam
Journal logs show "Detected change to /run/current-system" every ~10s on idle systems. Two problems:
- agent.rs prints the message for ANY file event in /run (not just current-system), then the guard returns early — misleading log.
- Legitimate events can fire multiple times for the same store path if GC roots/symlinks are touched without the target changing.
- Each false trigger sends an unnecessary heartbeat POST to the server.

### Issue 3: Agent restart vs system restart not distinguished
Agent restart reports change_reason: "startup" — same context as system reboots. Operators can't tell from the UI/API whether the system rebooted or just the agent restarted. Host uptime_secs IS collected but not surfaced for this distinction.

### Issue 4: Configurable heartbeat interval not end-to-end
Agent hardcodes sleep(600s). UI hardcodes 60s for countdown. Edit System modal has a selector (30/60/90/120/300s) but it is local-only — NOT in UpdateSystemRequest, no DB column, nothing reaches the agent. Help text says "Not saved yet — backend field coming soon."

## Goal

1. Eliminate false-positive "offline" states: resilient heartbeat retry, better error surfacing, heartbeat health telemetry.
2. Eliminate false change-detection spam and prevent unnecessary heartbeats from duplicate/noop events.
3. Distinguish agent restarts from system reboots so operators correctly assess fleet health.
4. Persist per-system heartbeat interval from Edit System modal and have the agent apply it — end-to-end.

## Non-Goals

- No alerting or incident-driven interval changes.
- No global server-wide override UI (server-config default as fallback is enough).
- No agent deployment/switch logic changes beyond sleep duration.
- No free-form per-second input; keep discrete options (30/60/90/120/300) + agent default 600.
- No redesign of offline detection thresholds (15min/1hr/4hr unchanged).
- No new change_reason variant added unless uptime-based detection proves insufficient.

## Scope

### Agent Observability & Reliability
1. Fix misleading println in agent.rs — only print for actual "current-system" events.
2. Add inotify event deduplication: if /run/current-system resolved path hasn't changed since last heartbeat, suppress the POST.
3. Add jitter around sleep interval to prevent thundering herd.
4. Add client-side retry with exponential backoff for failed heartbeat POSTs.
5. Add warning log with HTTP status code/details on heartbeat POST failure.

### Agent Restart vs System Restart Detection
6. On agent startup, compare stored host uptime (from last heartbeat file) against current uptime. Lower uptime = system reboot. Otherwise = agent restart.
7. Include is_system_reboot: bool in heartbeat/system-state POST.
8. Surface restart_type in view_system_detail (system_reboot | agent_restart | unknown).

### Configurable Heartbeat Interval (End-to-End)
9. New migration: nullable systems.heartbeat_interval_secs integer with COMMENT. NULL = server/agent default.
10. Project into view_system_detail and view_system_list via CREATE OR REPLACE VIEW (append pattern).
11. Server config (config/server.rs) exposes heartbeat_interval_secs: u64 default 600.
12. UpdateSystemRequest (server + web-ui) gains heartbeat_interval_secs: Option<i32> with serde(default).
13. update_system_metadata writes heartbeat_interval_secs (0/NULL → default).
14. SystemDetailRow/ListRow and DTOs gain heartbeat_interval_secs; mapped from rows.
15. LogResponse gains heartbeat_interval_secs: Option<u64> with serde(default); populated from system value or config default.
16. process_heartbeat_response surfaces heartbeat_interval_secs from LogResponse to caller.
17. Agent sleep loop uses server-provided interval; falls back to 600 if absent. Both initial and loop sleep honor latest interval.
18. Edit System modal seeds heartbeat_interval_sec from system value (fallback 600), submits in UpdateSystemRequest, replaces "Not saved yet" help text.
19. HeartbeatSpinner in system_detail, system_card_v2, systems_table, systems_list uses heartbeat_interval_secs instead of hardcoded 60.

## Architectural Constraints

- New wire/DTO fields MUST be serde(default) for backwards compatibility.
- Agent fallback when field absent MUST remain 600s.
- Server-config default MUST be 600; per-system NULL falls back to it.
- Web UI must not break if field absent from API response.
- Schema changes MUST use NEW migration; existing migrations NOT modified.
- Debounce suppresses only identical store paths, not all heartbeats.
- Inotify watcher stays on /run; only the log message is fixed.
- SQLx metadata regenerated; cargo sqlx prepare --check must pass.

## Verification Plan

Tier 0:
- cargo fmt -- --check
- cargo clippy -- -D warnings on affected crates
- cargo check --manifest-path packages/default/Cargo.toml --all-targets
- cargo check --manifest-path packages/web-ui/Cargo.toml --target wasm32-unknown-unknown --all-targets
- cargo test on affected crates
- node --check checks/web-ui/tests/integration-test.js

Tier 1:
- Isolated local dev DB; apply migration; cargo sqlx prepare --check
- Agent test: sleep loop uses server interval, falls back to 600
- Agent test: inotify debounces identical store paths
- Server test: LogResponse.heartbeat_interval_secs reflects persisted value
- Agent test: is_system_reboot true when uptime < agent runtime
- Seed a system with known interval; update via Edit modal; verify countdown reflects new interval

Tier 2:
- nix flake check (or nix build .#checks.x86_64-linux.web-ui) before Review
- web-ui check captures Edit modal persistence and spinner countdown

## Impact Areas

- packages/default/src/bin/agent.rs
- packages/default/src/config/server.rs
- packages/default/migrations/<next>_add_system_heartbeat_interval.sql
- packages/default/src/handlers/agent/heartbeat.rs
- packages/default/src/deployment/agent.rs
- packages/default/src/api/models.rs
- packages/default/src/queries/systems.rs
- packages/default/src/services/systems.rs
- packages/default/src/models/system_states.rs
- packages/default/.sqlx/*
- packages/web-ui/src/api/models.rs
- packages/web-ui/src/components/system/edit_system_modal.rs
- packages/web-ui/src/components/system/system_card_v2.rs
- packages/web-ui/src/components/tables/systems_table.rs
- packages/web-ui/src/views/system_detail.rs
- packages/web-ui/src/views/systems_list.rs
- checks/web-ui/tests/integration-test.js

## Risk Level

High. Cross-cuts agent, server, DB, DTOs, and UI. Backwards-compatibility via serde(default) and 600s fallback on all new fields. Main risks: SQLx metadata drift, un-upgraded agents (mitigated by optional fields), test-only struct initializers (mitigated by --all-targets).

## Dependencies

- Builds on TASK-353's editable per-system field patterns.
- Subsumes TASK-279's specification (heartbeat interval via LogResponse).
- Migration number must follow latest applied (currently 0137).
<!-- SECTION:DESCRIPTION:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
LOCK: ai-agent on mcamp-workstation in ~/code/crystal-forge/TASK-378-agent-observability-heartbeat

Review feedback (b85b18d9) addressed:
- P1: startup interval discarded → ALREADY FIXED in bcf37809 via watch::channel
- P1: failed heartbeat poisons dedup → ALREADY FIXED in bcf37809 via HeartbeatResult::Failed
- P1: migration 0146 changed health thresholds to interval-derived → FIX PENDING: new migration 0147 to restore fixed 15min/1hr/4hr thresholds
- P2: response deserialization retried → ALREADY FIXED in bcf37809
- P2-7: UI 'Use server default' sends Set(600) instead of Clear → FIX PENDING
- P2 boot-ID: server logs change but doesn't persist system_reboot reason → addressed by boot_id column; authoritative classification noted

Constraint: This is deployed to dev server - DO NOT modify existing migrations, only add new ones (0147+)
<!-- SECTION:NOTES:END -->

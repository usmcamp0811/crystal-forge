---
id: TASK-279
title: Server-directed agent heartbeat interval via LogResponse
status: Backlog
assignee:
  - '@ai-agent'
created_date: '2026-04-20 03:01'
labels:
  - agents
  - heartbeat
  - server
  - api
  - web-ui
  - observability
milestone: UI/UX Design System
dependencies: []
references:
  - /home/mcamp/code/crystal-forge/dev/packages/default/src/bin/agent.rs
  - >-
    /home/mcamp/code/crystal-forge/dev/packages/default/src/handlers/agent/heartbeat.rs
  - /home/mcamp/code/crystal-forge/dev/packages/default/src/deployment/agent.rs
  - /home/mcamp/code/crystal-forge/dev/packages/default/src/config/mod.rs
documentation:
  - /home/mcamp/code/crystal-forge/dev/packages/default/src/api/models.rs
  - /home/mcamp/code/crystal-forge/dev/packages/default/src/queries/systems.rs
priority: medium
ordinal: 4100
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

The agent currently heartbeats on a hardcoded 10-minute timer (`sleep(600)` in `bin/agent.rs`). The server never tells the agent when to come back. This means:

- Operators cannot tune heartbeat frequency without redeploying the agent binary
- The UI cannot show a meaningful countdown because no reliable interval is known
- There is no mechanism to increase check-in frequency during incidents, deployments, or policy changes

## Goal

Make the server tell every agent how long to wait before its next heartbeat, via the existing `LogResponse` struct that is already parsed by the agent after each heartbeat POST. The agent should sleep for that server-provided interval instead of its current hardcoded constant.

This is the correct place to wire this because:
- `LogResponse` is already the agent's instruction envelope (it carries `desired_target` and `runtime_caches` today)
- No DB migrations are required — the interval lives in server config
- Adding `next_heartbeat_secs` to `LogResponse` is backwards-compatible (agents that don't read it fall back gracefully)
- It allows real-time tuning: the server can return a shorter interval during deployment or incidents by adjusting config without touching agents

## Non-Goals

- Does NOT require a new database migration
- Does NOT change agent deployment logic
- Does NOT add per-system overrides (follow-up if needed)
- Does NOT implement alerting or automated interval changes

## Scope

1. Add `next_heartbeat_secs: Option<u64>` to `LogResponse` in `handlers/agent/heartbeat.rs`
2. Add `heartbeat_interval_secs: u64` (default `600`) to server config (`config/mod.rs` or relevant config struct)
3. Server handler populates `next_heartbeat_secs` from config in every `LogResponse`
4. Agent `run_periodic_heartbeat_loop_with_deployment` in `bin/agent.rs` reads the returned interval from `process_heartbeat_response` and uses it for `sleep()`; falls back to `600` if absent
5. `process_heartbeat_response` in `deployment/agent.rs` surfaces `next_heartbeat_secs` from `LogResponse` to its caller
6. Web UI `SystemDetail` API response adds `heartbeat_interval_secs: Option<u64>` to `SystemDetail` DTO (server-side models + web-ui client models), populated from the agent's last-seen value or server default
7. Web UI `HeartbeatSpinner` in system detail and list views use the real interval when available instead of the hardcoded `60`

## Architectural Constraints

- `next_heartbeat_secs` must be `#[serde(default)]` so older agents that don't read it remain fully functional
- Agent fallback interval when field is absent must remain `600` (current behavior)
- Server config default must be `600` (no behavior change unless operator explicitly changes it)
- Web UI must never break if the field is absent from the API response
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Server `LogResponse` includes `next_heartbeat_secs: Option<u64>` field populated from server config (default 600).
- [ ] #2 Agent `run_periodic_heartbeat_loop_with_deployment` sleeps for the server-provided interval; falls back to 600 if field absent.
- [ ] #3 `process_heartbeat_response` surfaces the server interval to its caller so the loop can use it.
- [ ] #4 Server config exposes a `heartbeat_interval_secs` setting with a default of 600 and a comment explaining its purpose.
- [ ] #5 `SystemDetail` API response and web-ui DTO both include `heartbeat_interval_secs: Option<u64>` populated from the server default or last-seen agent value.
- [ ] #6 Web UI `HeartbeatSpinner` in system detail metric strip, system detail overview panel, system card, and systems table all use `heartbeat_interval_secs` when available instead of the hardcoded 60-second constant.
- [ ] #7 Existing agent and server tests still pass with no behavior change when `next_heartbeat_secs` is absent from the response.
<!-- AC:END -->

## Definition of Done
<!-- DOD:BEGIN -->
- [ ] #1 No DB migrations introduced
- [ ] #2 cargo fmt -- --check passes
- [ ] #3 cargo clippy -- -D warnings passes on affected crates
- [ ] #4 cargo test passes on affected crates
- [ ] #5 Agent integration test confirms interval respected after server change
- [ ] #6 Web UI HeartbeatSpinner uses real interval end-to-end on system detail view
<!-- DOD:END -->

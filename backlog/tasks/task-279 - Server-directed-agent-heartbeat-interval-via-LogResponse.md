---
id: TASK-279
title: Server-directed agent heartbeat interval via LogResponse
status: Backlog
assignee:
  - '@ai-agent'
created_date: '2026-04-20 03:01'
updated_date: '2026-06-14 13:48'
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
  - packages/default/src/bin/agent.rs
  - packages/default/src/handlers/agent/heartbeat.rs
  - packages/default/src/deployment/agent.rs
  - packages/default/src/config/mod.rs
  - packages/default/src/queries/systems.rs
  - packages/default/src/api/models.rs
  - packages/web-ui/src/components/system/edit_system_modal.rs
  - packages/web-ui/src/components/system/system_card_v2.rs
  - packages/web-ui/src/components/tables/systems_table.rs
  - TASK-353
documentation:
  - /home/mcamp/code/crystal-forge/dev/packages/default/src/api/models.rs
  - /home/mcamp/code/crystal-forge/dev/packages/default/src/queries/systems.rs
priority: medium
ordinal: 4100
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

The agent heartbeats on a hardcoded 600s (10-minute) timer (sleep(Duration::from_secs(600)) in bin/agent.rs:335 and :348). The server never tells the agent when to come back. Consequences:

- Operators cannot tune heartbeat frequency without redeploying the agent binary.
- The Edit System modal exposes a "Heartbeat interval" selector (30/60/90/120/300s) but it is local-only UI state. It is NOT included in UpdateSystemRequest, there is NO systems.heartbeat_interval_secs column, and nothing reaches the agent. The control is currently labeled "Not saved yet - backend field coming soon" so operators are not misled (TASK-353).
- The UI HeartbeatSpinner hardcodes a 60s interval (e.g. system_card_v2.rs heartbeat_interval_sec = 60_i64), so countdowns are not meaningful.
- There is no mechanism to increase check-in frequency during incidents, deployments, or policy changes.

## Goal

Make the per-system heartbeat interval configured in the Edit System modal authoritative end-to-end:

1. Persist the per-system interval in the database.
2. Return it to the agent via the existing LogResponse instruction envelope after each heartbeat POST.
3. Have the agent sleep for that server-provided interval (falling back to the existing 600s if absent).
4. Surface the effective interval in SystemDetail so the UI HeartbeatSpinner shows a real countdown.

LogResponse is the correct transport because it is already the agent's per-heartbeat instruction envelope (it carries desired_target and runtime_caches today) and adding an optional field is backwards-compatible.

## Goal (one sentence)

A heartbeat interval selected in the Edit System modal is persisted, returned to that system's agent on its next heartbeat, applied by the agent's sleep loop, and reflected by the UI heartbeat countdown.

## Non-Goals

- Does NOT implement alerting or automated/incident-driven interval changes (server returns the configured value only).
- Does NOT add a global server-wide override UI (a server-config default is acceptable as the fallback source, but the primary control is per-system).
- Does NOT change agent deployment/switch logic beyond the sleep duration.
- Does NOT add free-form per-second input; keep the existing discrete options (30/60/90/120/300) plus the agent default.

## Scope

### Database
1. New migration 0138_add_system_heartbeat_interval.sql (next free number; do NOT edit applied migrations) adding nullable systems.heartbeat_interval_secs integer with a COMMENT. NULL means "use server/agent default".
2. Project s.heartbeat_interval_secs into view_system_detail by appending it (CREATE OR REPLACE VIEW, append column to avoid column-order errors). Also project into view_system_list via the same append pattern since the spinner is on cards/table.

### Backend
3. UpdateSystemRequest (server api/models.rs and web-ui api/models.rs) gains heartbeat_interval_secs: Option<i32> (serde(default)).
4. update_system_metadata in queries/systems.rs writes heartbeat_interval_secs (treat 0/empty as NULL -> default).
5. SystemDetailRow and SystemListRow gain heartbeat_interval_secs: Option<i32>; map into SystemDetail/SystemSummary DTOs (server + web-ui).
6. LogResponse in handlers/agent/heartbeat.rs gains next_heartbeat_secs: Option<u64> (serde(default)). The heartbeat handler populates it from the system's heartbeat_interval_secs when set, else the server-config default.
7. Server config (config/mod.rs or relevant struct) exposes heartbeat_interval_secs: u64 default 600, with a comment. This is the fallback when a system has no per-system value.

### Agent
8. process_heartbeat_response in deployment/agent.rs surfaces next_heartbeat_secs from LogResponse to its caller without breaking the existing return contract.
9. run_periodic_heartbeat_loop_with_deployment in bin/agent.rs uses the returned interval for sleep(); falls back to 600 if absent. Both the initial sleep and loop sleep should honor the latest known interval.

### Web UI
10. Edit System modal (components/system/edit_system_modal.rs): seed heartbeat_interval_sec from system.heartbeat_interval_secs (fall back to current default), include it in the submitted UpdateSystemRequest, and replace the "Not saved yet" help text with accurate copy describing that it takes effect on the agent's next check-in.
11. HeartbeatSpinner usages in system detail metric strip, system detail overview panel, system_card_v2.rs, and systems_table.rs use heartbeat_interval_secs when available instead of the hardcoded 60.

## Architectural Constraints

- next_heartbeat_secs and heartbeat_interval_secs wire/DTO fields MUST be serde(default) so older agents/clients remain fully functional.
- Agent fallback interval when the field is absent MUST remain 600 (no regression for un-upgraded agents).
- Server-config default MUST be 600; per-system NULL falls back to it.
- Web UI must never break if the field is absent from the API response.
- Schema changes MUST use a NEW migration file; existing migrations (incl. 0135-0137) MUST NOT be edited (dev server has them applied).
- SQLx offline metadata MUST be regenerated (cargo sqlx prepare) against an isolated local dev DB and committed; cargo sqlx prepare --check MUST pass.
- No business logic in UI views; DTOs mirror server models.

## Verification Plan

Tier 0 (local, required):
- cargo fmt --manifest-path packages/web-ui/Cargo.toml --all -- --check
- cargo check --manifest-path packages/web-ui/Cargo.toml --target wasm32-unknown-unknown --all-targets
- Backend with Nix OpenSSL/pkg-config: cargo check --manifest-path packages/default/Cargo.toml --all-targets (MUST include --all-targets to catch test-only builders like SystemSummaryBuilder).
- cargo clippy on affected crates with -D warnings.
- Apply migration to isolated local dev DB (port 3042), run cargo sqlx prepare + cargo sqlx prepare --check.
- node --check checks/web-ui/tests/integration-test.js.

Tier 1 (feature-level):
- Add/adjust an agent unit/integration test asserting the loop sleeps for the server-provided interval and falls back to 600 when next_heartbeat_secs is absent.
- Add a server handler test asserting LogResponse.next_heartbeat_secs reflects the system's persisted value (and the config default when NULL).

Tier 2 (Nix integration): Run nix flake check (or at least nix build .#checks.x86_64-linux.web-ui) before marking Review, since this touches agent, server, DTOs, migration, and UI across crates. The UI change MUST be captured by the web-ui check (assert the Edit modal persists the interval and the spinner reflects it).

## Impact Areas

- packages/default/migrations/0138_add_system_heartbeat_interval.sql (new)
- packages/default/src/handlers/agent/heartbeat.rs (LogResponse + populate)
- packages/default/src/deployment/agent.rs (process_heartbeat_response surfaces interval)
- packages/default/src/bin/agent.rs (sleep loop honors interval)
- packages/default/src/config/mod.rs (server default)
- packages/default/src/api/models.rs (UpdateSystemRequest, SystemDetail, SystemSummary)
- packages/default/src/queries/systems.rs (update_system_metadata, SystemDetailRow, SystemListRow)
- packages/default/src/services/systems.rs (row -> DTO mapping)
- packages/default/.sqlx/* (regenerated metadata)
- packages/web-ui/src/api/models.rs (DTO mirrors)
- packages/web-ui/src/components/system/edit_system_modal.rs (seed + submit + help text)
- packages/web-ui/src/components/system/system_card_v2.rs, packages/web-ui/src/components/tables/systems_table.rs, packages/web-ui/src/views/system_detail.rs (HeartbeatSpinner interval source)
- checks/web-ui/tests/integration-test.js (assert persistence + spinner)

## Risk Level

Medium. Cross-cuts agent, server, DB, DTOs, and UI. Backwards-compatibility hinges on every new wire field being serde(default) and the agent retaining the 600s fallback. Main risks: SQLx metadata drift (mitigated by prepare/check), un-upgraded agents (mitigated by optional field + fallback), and forgetting test-only struct initializers (mitigated by --all-targets).

## Dependencies

- Builds on TASK-353's editable per-system fields (FQDN) which established the UpdateSystemRequest -> update_system_metadata -> view projection pattern and the new-migration approach (0136/0137). TASK-279 follows the same pattern for heartbeat_interval_secs.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 New migration adds nullable systems.heartbeat_interval_secs (integer) with a comment; existing migrations are not modified
- [ ] #2 view_system_detail and view_system_list project heartbeat_interval_secs via CREATE OR REPLACE VIEW appending the column
- [ ] #3 UpdateSystemRequest (server + web-ui) includes heartbeat_interval_secs: Option<i32> with serde(default); update_system_metadata persists it with 0/empty stored as NULL
- [ ] #4 SystemDetailRow/SystemListRow and SystemDetail/SystemSummary DTOs (server + web-ui) include heartbeat_interval_secs and are mapped from rows
- [ ] #5 Server LogResponse includes next_heartbeat_secs: Option<u64> with serde(default) populated from the system's persisted interval or the server-config default of 600
- [ ] #6 Server config exposes heartbeat_interval_secs default 600 with an explanatory comment used as fallback when a system value is NULL
- [ ] #7 process_heartbeat_response surfaces next_heartbeat_secs to its caller without breaking the existing return contract
- [ ] #8 Agent run_periodic_heartbeat_loop_with_deployment sleeps for the server-provided interval and falls back to 600 when the field is absent
- [ ] #9 Edit System modal seeds the interval from system.heartbeat_interval_secs submits it in UpdateSystemRequest and replaces the Not saved yet help text with accurate copy
- [ ] #10 HeartbeatSpinner in system detail metric strip overview panel system card and systems table use heartbeat_interval_secs when available instead of the hardcoded 60
- [ ] #11 SQLx offline metadata regenerated and committed; cargo sqlx prepare --check passes
- [ ] #12 Existing agent and server tests pass unchanged when next_heartbeat_secs is absent; new tests assert interval is respected and falls back to 600
- [ ] #13 checks/web-ui asserts the Edit modal persists the interval and the heartbeat countdown reflects it
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

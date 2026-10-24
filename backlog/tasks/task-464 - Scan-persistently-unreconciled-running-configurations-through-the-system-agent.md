---
id: TASK-464
title: Scan persistently unreconciled running configurations through the system agent
status: Backlog
assignee: []
created_date: '2026-09-20 17:32'
labels:
  - backend
  - cve
  - agent
  - scanning
  - security
  - scheduler
milestone: Security Operations
dependencies: []
references:
  - packages/default/crates/cf-server/src/queries/scanning.rs
  - packages/default/crates/cf-server/src/handlers/api/scanning.rs
  - packages/default/crates/cf-builder/src/builder/cve_scanner.rs
  - packages/default/crates/cf-server/src/queries/cve_scan_leases.rs
  - packages/default/crates/cf-agent/src/system_state.rs
  - packages/default/crates/cf-protocol/src/agent.rs
  - packages/default/crates/cf-protocol/src/builder.rs
  - packages/default/crates/cf-server/src/handlers/agent/heartbeat.rs
  - packages/default/crates/cf-server/src/queries/system_states.rs
  - packages/default/crates/cf-server/src/queries/system_events.rs
  - packages/default/crates/cf-server/src/tasks/attention_reconciliation.rs
  - >-
    packages/default/crates/cf-server/migrations/0134_add_scan_schedule_policy.sql
  - docs/agent-heartbeat-state-history.md
  - docs/multi-builder-api.md
  - >-
    backlog/tasks/task-325 - Restore automated CVE scans and functional fleet
    rescan
  - >-
    backlog/tasks/task-415 - Implement deployment approvals and running-state
    attestations
  - 'backlog/tasks/task-336.6 - Admin Server: agent heartbeat config API'
priority: high
type: feature
ordinal: 482000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Crystal Forge monitors systems through agents. Normally the server reconciles a system's reported running NixOS generation/store path to known Crystal Forge state (evaluated configuration, NixOS derivation, retained generation/evaluation snapshot, Crystal Forge deployment, exact schema-1 CVE evidence). However, a monitored system can legitimately run a configuration that Crystal Forge cannot currently reconcile (e.g., local `nixos-rebuild switch`, out-of-band deployment, server evaluation still catching up, reconciliation process lag, previously unknown revision). This state is not necessarily prohibited by organizational policy.

If a system remains unreconciled for a meaningful period, operators still need CVE visibility for the ACTUAL RUNNING SYSTEM.

Goal: If a system has remained unreconciled for longer than a configurable grace period, have THAT SYSTEM'S AGENT scan its actual running configuration and report the result to Crystal Forge. This gives security operators CVE awareness for an otherwise unknown running configuration, allowing them to decide whether investigation, isolation, quarantine, remediation, or some other response is appropriate. Actual quarantine enforcement is NOT part of this task.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Persistent unreconciled identity is detected server-side and has durable first_seen_unreconciled timestamp
- [ ] #2 Reconciliation before grace expiry produces no runtime scan
- [ ] #3 A continuously unreconciled system becomes eligible after effective policy grace expires
- [ ] #4 Effective policy is global-default + optional per-environment override
- [ ] #5 Disabled policy prevents automatic runtime scanning
- [ ] #6 Server issues work only to the affected registered agent
- [ ] #7 Old/non-capable agents degrade cleanly and report capability state
- [ ] #8 Agent scans the exact live runtime identity without evaluating/rebuilding a substitute configuration
- [ ] #9 Runtime identity change during scan prevents misattribution
- [ ] #10 Results are immutable/bounded and carry explicit agent-runtime provenance
- [ ] #11 Agent-runtime evidence cannot satisfy exact CVE authority or exact POA&M / verification / closure contracts
- [ ] #12 Repeated heartbeats do not reset grace
- [ ] #13 A new runtime store identity starts a new grace episode
- [ ] #14 Successful reconciliation stops future scans for that episode
- [ ] #15 Still-unreconciled systems rescan according to the repeat interval
- [ ] #16 No duplicate active scan exists for one system + runtime identity
- [ ] #17 Server restart and agent restart preserve correct idempotent behavior
- [ ] #18 Failed/missing scanner state is visible and retry behavior is bounded
- [ ] #19 High/critical runtime findings are exposed to existing attention/security operations without automatically quarantining the system
- [ ] #20 Historical runtime results remain queryable after reconciliation or later configuration changes
- [ ] #21 Existing builder schema-1 exact scan behavior remains unchanged
- [ ] #22 Existing exact-current CVE mutation authority remains fail closed
- [ ] #23 Unit/database/protocol/integration coverage proves all lifecycle and authority boundaries
- [ ] #24 Web UI work is explicitly blocked until updated maintainer design examples are supplied, then browser coverage/screenshots are required according to repository standards
<!-- AC:END -->

## Definition of Done
<!-- DOD:BEGIN -->
- [ ] #1 All acceptance criteria verified with automated tests
- [ ] #2 Migration applied and SQLx offline metadata updated
- [ ] #3 No `allow(missing_docs)` added to avoid documenting new public items
- [ ] #4 All new public Rust items have useful rustdoc with required sections
- [ ] #5 Significant unsafe blocks have local SAFETY comments
- [ ] #6 Documentation standard from AGENTS.md satisfied (contracts, invariants, state transitions, side effects, error conditions, concurrency, persistence, authorization, compatibility, units/ranges, non-obvious rules, performance tradeoffs)
- [ ] #7 Stale comments removed or corrected
- [ ] #8 No untracked TODO/FIXME introduced
- [ ] #9 Rustfmt and clippy clean on changed files
- [ ] #10 Exact-head CI green for affected packages
- [ ] #11 MR screenshots attached for any UI changes (after design unblock)
- [ ] #12 Preview URL reported and verified for browser-visible changes
<!-- DOD:END -->

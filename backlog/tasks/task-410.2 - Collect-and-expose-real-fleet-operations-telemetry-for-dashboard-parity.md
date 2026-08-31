---
id: TASK-410.2
title: Collect and expose real fleet operations telemetry for dashboard parity
status: To Do
assignee: []
created_date: '2026-08-31 02:21'
labels:
  - dashboard
  - agent
  - telemetry
  - api
  - design-parity
  - real-data
dependencies: []
references:
  - git commit ac582592e8ffd787f103578c272d9f30162a9480
  - TASK-410.1
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/320'
documentation:
  - docs/design/CrystalForge/data-fleet-ops.js
  - docs/design/CrystalForge/components/DashboardWidgetsOps.jsx
  - docs/design/CrystalForge/components/DashboardView.jsx
modified_files:
  - packages/default/crates/cf-agent/
  - packages/default/crates/cf-protocol/
  - packages/default/crates/cf-server/
  - packages/web-ui/src/api/models.rs
parent_task_id: TASK-410
priority: high
type: feature
ordinal: 453000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The fleet-operations widgets added in design commit ac582592 require authoritative host and historical data that Crystal Forge does not currently collect or expose. Add real, bounded, authorization-scoped product data for configuration drift, closure and disk pressure, rollback readiness, reboot requirements, 14-day deploy state, and the 365-day fleet calendar. Agent-reported values must describe the host's actual state; the server must persist and aggregate them without deterministic mock values or client-side fabrication. Missing or unsupported observations remain explicitly unknown. This is the backend follow-up to TASK-410 and must preserve deployed agent compatibility through an additive transition.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The product exposes each system's current numeric distance from its tracked flake HEAD when the running revision and tracked HEAD are known and reports unknown otherwise
- [ ] #2 Agent telemetry reports actual filesystem capacity free space Nix store usage current closure size retained on-disk generation count and last garbage-collection observation with explicit unknown values for unavailable measurements
- [ ] #3 Rollback readiness distinguishes a server-recorded historical generation from a previous generation that is confirmed to remain available on the host
- [ ] #4 Reboot-required telemetry compares the booted and activated kernel kernel-modules and initrd identities and reports the exact reason or unknown without treating unreadable state as no reboot required
- [ ] #5 The server provides bounded visibility-scoped current fleet-operation summaries plus a 14-day deploy-state history and a 365-day compliance/drift calendar derived only from persisted observations and events
- [ ] #6 Historical gaps and systems without sufficient observations remain visibly unknown and are not backfilled with synthetic values
- [ ] #7 Telemetry ingestion validates units and ranges limits payload size and preserves compatibility with supported deployed agents during rollout
- [ ] #8 Non-admin users receive only systems and aggregate values allowed by existing environment visibility rules and hidden systems cannot influence their results
- [ ] #9 Schema and query changes use additive migrations and matching SQLx metadata where required
- [ ] #10 Focused agent protocol persistence aggregation authorization compatibility and empty-state tests pass through the repository Nix environment and affected contracts are documented
<!-- AC:END -->

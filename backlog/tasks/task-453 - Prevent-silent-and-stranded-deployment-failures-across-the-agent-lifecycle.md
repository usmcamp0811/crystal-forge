---
id: TASK-453
title: Prevent silent and stranded deployment failures across the agent lifecycle
status: To Do
assignee: []
created_date: '2026-08-30 14:25'
labels:
  - deployment
  - agent
  - backend
  - reliability
  - observability
dependencies: []
references:
  - TASK-242
  - TASK-250
  - TASK-437
  - packages/default/crates/cf-agent/src/deployment/agent.rs
  - packages/default/crates/cf-agent/src/bin/agent.rs
  - packages/default/crates/cf-server/src/handlers/agent/heartbeat.rs
  - packages/default/crates/cf-server/src/handlers/agent/deployment_started.rs
  - packages/default/crates/cf-server/src/handlers/agent/deployment_failed.rs
  - packages/default/crates/cf-server/src/queries/system_events.rs
  - packages/default/crates/cf-server/src/queries/systems.rs
documentation:
  - docs/deployments_design_doc.md
  - docs/agent/database-safety.md
  - docs/agent/verification.md
modified_files:
  - packages/default/crates/cf-agent/src/deployment/agent.rs
  - packages/default/crates/cf-agent/src/bin/agent.rs
  - packages/default/crates/cf-server/src/handlers/agent/**
  - packages/default/crates/cf-server/src/queries/system_events.rs
  - packages/default/crates/cf-server/src/queries/systems.rs
  - packages/default/crates/cf-server/migrations/**
  - packages/default/crates/cf-server/tests/**
  - packages/cf-test-suite/**
  - docs/deployments_design_doc.md
priority: high
type: bug
ordinal: 451000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

A deployment can remain visible as `picked_up` or `applying` without a terminal result, current diagnostic evidence, or an explanation for the operator. The reported `blue-ridge` deployment remained at “Agent fetched the deployment command” while the available system logs were approximately 15 days old.

Static review of `dev` found several backend lifecycle gaps that can produce this behavior:

- The server records `delivered_at` before the heartbeat response reaches or is accepted by the agent, but the API presents that state as `picked_up`.
- Cache-copy work and related diagnostics remain local to the agent journal. Long-running copy work also blocks normal heartbeat processing.
- Agent-side failure reporting is best-effort and can be lost during a network error, restart, reboot, or service replacement.
- Activation runs in a detached systemd unit whose terminal result is not reported.
- Pending deployments use a fixed expiry, have no renewable progress lease, and are not actively reconciled into an explained terminal failure when work stalls or the agent disconnects.
- Retrying the same target can reuse an old pending row and its stale delivery/applying timestamps instead of creating a fresh attempt.
- Late success or failure reports can be ignored after expiry, which can leave the recorded outcome incorrect.

## Goal

Make every deployment attempt converge to a truthful, durable backend state. Crystal Forge must distinguish dispatch from agent acceptance, retain actionable failure context, detect abandoned work, and expose a terminal reason through existing deployment APIs and event/notification mechanisms. External cache or network failures can remain outside Crystal Forge's control, but Crystal Forge must not fail silently when they prevent deployment.

## Scope

- Agent/server deployment-attempt correlation and lifecycle persistence.
- Reliable acceptance, progress, success, failure, timeout, restart, and disconnect handling.
- Cache-fetch/copy and activation failure capture with bounded, redacted diagnostic context.
- Active stale-attempt reconciliation and fresh same-target retries.
- Backend API, event, audit, and notification correctness.
- Regression and integration coverage for the affected lifecycle.
- Required protocol and deployment-lifecycle documentation.

## Non-goals

- No redesign of System Detail or other UI/UX surfaces.
- No new deploy-timeline step or visual treatment; that work will be tracked separately.
- No binary-cache or network infrastructure redesign.
- No post-deploy application/service health validation; TASK-250 owns that separate concern.
- No change to newest-deployable target selection or behind-status semantics; TASK-437 owns that separate concern.

## Compatibility and safety constraints

- Preserve compatibility with supported deployed agents during any protocol transition, with explicit handling for legacy reports.
- Keep agent authentication and server-issued deployment authorization enforced.
- Never persist cache credentials, authorization headers, signed URLs, or unbounded command output.
- Deployment result and retry processing must be idempotent and safe across duplicate or out-of-order reports.

## Risk

High. This task changes agent/server deployment coordination, durable state transitions, failure reporting, and recovery behavior.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Each deployment request has a durable attempt identity that correlates agent acceptance, progress, success, failure, timeout, and retry reports without relying only on hostname and target path.
- [ ] #2 The backend does not report `picked_up` or an equivalent accepted state until the agent explicitly acknowledges the deployment attempt; dispatch alone remains distinguishable from acceptance.
- [ ] #3 Cache fetch/copy failures, local process failures, activation submission failures, and detached activation failures produce a durable terminal failure with a bounded and redacted reason.
- [ ] #4 Deployment result reporting survives transient network failure and agent restart or reboot through retry or reconciliation, and duplicate or out-of-order reports cannot corrupt a terminal outcome.
- [ ] #5 Long-running deployment work does not prevent the agent from maintaining health heartbeats or reporting bounded deployment progress.
- [ ] #6 Active deployment attempts use explicit inactivity/expiry semantics that are compatible with configured execution and retry limits; a valid in-progress attempt cannot silently outlive its backend record.
- [ ] #7 A server-side reconciliation path converts abandoned queued, accepted, or applying attempts into an explained terminal state and distinguishes agent disconnection from deployment progress timeout when evidence permits.
- [ ] #8 Expired or late terminal reports are reconciled to the correct attempt within a documented bounded policy instead of being accepted as an unexplained no-op.
- [ ] #9 A manual retry of the same target creates a fresh attempt with fresh lifecycle timestamps and does not reuse stale `picked_up` or `applying` state.
- [ ] #10 An already-current target terminalizes the matching active attempt successfully without leaving a pending row.
- [ ] #11 Existing deployment status, system-event, audit, and notification APIs expose the persisted terminal reason so current consumers can explain the failure without a UI redesign.
- [ ] #12 Supported deployed agent versions remain compatible during rollout, and the compatibility behavior and removal conditions for any legacy path are documented.
- [ ] #13 Backend and agent tests cover cache/network/process failures, lost reports, agent restart/disconnect, detached activation failure, stale-attempt reconciliation, same-target retry, late reports, and successful deployment.
- [ ] #14 Integration verification proves that every exercised deployment attempt reaches a durable success or explained failure/timeout state and that secrets are absent from persisted diagnostics.
- [ ] #15 Deployment lifecycle documentation defines state meanings, valid transitions, timeout/retry behavior, idempotency, correlation, redaction, and legacy-agent compatibility.
<!-- AC:END -->

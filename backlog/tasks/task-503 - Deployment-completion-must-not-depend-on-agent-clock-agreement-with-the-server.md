---
id: TASK-503
title: Deployment completion must not depend on agent clock agreement with the server
status: Backlog
assignee: []
created_date: '2026-10-23 23:31'
labels:
  - bug
  - deployment
  - agent
  - integration
dependencies: []
references:
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/335'
  - packages/default/crates/cf-server/src/queries/system_events.rs
  - packages/default/crates/cf-agent/src/system_state.rs
priority: high
type: bug
ordinal: 554000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
A real target activated the deployed generation and reported it, but the deployment remained pending because the controller compared its own deployment issue time against the agent-supplied report timestamp. When the target clock was about 10 seconds behind the controller after a host CPU stall, the startup report appeared older than the deployment and was recorded as a local rebuild. Later heartbeats said the target was already on the requested configuration, so completion was never retried.

Discovered by MR !335 normal integration-assurance at 642db0ec (case cve-vulnerable-actual-ui-deployment-and-agent-observation, artifact /nix/store/2rpai6p3vnpk7gq5zd8h7jn87ihpb108-vm-test-run-cf-it-assurance-evidence-remediation-lifecycle-run). Source: packages/default/crates/cf-server/src/queries/system_events.rs MATCH_REPORTED_DEPLOYMENT_SQL condition candidate.issued_at <= agent payload timestamp; agent timestamp from cf-agent system_state.rs. The condition predates MR !335 (TASK-440). Delivery activation and Upgrade version-transition scenarios use the same completion path, so all real-agent deployment proofs can fail nondeterministically under guest clock skew; real fleets without tight clock sync are also affected.

Changing which reports may complete a deployment changes deployment attribution and requires an owner decision on the comparison rule before implementation.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Owner-approved attribution rule is recorded before implementation, including whether a report generated before issue but received after issue may complete the deployment
- [ ] #2 A real agent report of the exact requested store path completes the matching pending deployment when the agent clock is behind or ahead of the server by a bounded skew
- [ ] #3 A report of a different store path or a superseded or expired deployment still does not complete it
- [ ] #4 A focused database regression proves completion with a skewed agent timestamp and preserves existing local-rebuild classification cases
- [ ] #5 Agent-supplied event times remain recorded as agent observations and are documented as untrusted for ordering against server time
- [ ] #6 Delivery, Assurance and Upgrade owner deployment observations pass without timing or clock workarounds in the scenarios
- [ ] #7 Affected knowledge documentation describes the clock-domain rule
<!-- AC:END -->

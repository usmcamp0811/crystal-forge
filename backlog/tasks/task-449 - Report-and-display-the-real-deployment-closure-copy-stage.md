---
id: TASK-449
title: Report and display the real deployment closure-copy stage
status: To Do
assignee: []
created_date: '2026-08-31 02:22'
labels:
  - agent
  - server
  - deployments
  - web-ui
  - state-machine
  - design-parity
dependencies: []
references:
  - git commit ac582592e8ffd787f103578c272d9f30162a9480
documentation:
  - docs/design/CrystalForge/components/SystemDetail.jsx
  - docs/design/CrystalForge/components/Systems.jsx
modified_files:
  - packages/default/crates/cf-agent/
  - packages/default/crates/cf-protocol/
  - packages/default/crates/cf-server/src/
  - packages/web-ui/src/components/system/pending_deploy_banner.rs
  - packages/web-ui/src/components/heartbeat_spinner.rs
  - packages/web-ui/src/views/system_detail.rs
  - checks/web-ui/tests/integration-test.js
priority: high
type: feature
ordinal: 460000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The design now distinguishes closure transfer from activation, but Crystal Forge's authoritative deployment lifecycle currently jumps from picked_up to applying. Add a real copying stage that is emitted from agent/server progress rather than a frontend timer. This stage must make cache transfer stalls and failures observable without weakening deployment authorization, lease ownership, retry, timeout, or compatibility guarantees.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The authoritative deployment lifecycle includes a copying stage between picked up and applying only when closure transfer has actually begun
- [ ] #2 The agent reports copying progress through an additive protocol transition that remains compatible with supported agents that do not yet emit the stage
- [ ] #3 The server persists or derives copying state from real agent progress and never advances to copying from an elapsed frontend timer
- [ ] #4 Copy failures stalls cancellation retries lease loss and terminal transitions preserve existing deployment ownership and authorization invariants and do not strand a deployment
- [ ] #5 System deployment progress APIs document and return queued picked_up copying applying activated and failed semantics consistently
- [ ] #6 The pending-deploy banner and progress indicator render Copying from cache with truthful target and failure context and remain compatible with legacy progress records
- [ ] #7 A deployment that does not require copying may move directly from picked_up to applying and the UI does not fabricate a copy interval
- [ ] #8 Timeout and stale-progress behavior distinguishes an agent waiting to pick up work from one stalled while copying and supplies actionable diagnostics without exposing secrets or signed cache URLs
- [ ] #9 Focused protocol agent server state-machine compatibility and failure tests pass through the repository Nix environment
- [ ] #10 The authoritative web-ui check passes with assertion coverage and screenshots for copying direct-to-applying copy failure cancellation legacy-agent and terminal states
<!-- AC:END -->

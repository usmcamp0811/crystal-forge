---
id: TASK-452
title: Add fast persistent Web UI E2E development loop
status: To Do
assignee: []
created_date: '2026-09-01 21:01'
labels:
  - web-ui
  - developer-experience
  - e2e
dependencies: []
references:
  - checks/web-ui/tests/integration-test.js
  - checks/web-ui/default.nix
  - dev/packages/devScripts/
documentation:
  - docs/agent/verification.md
  - docs/agent/worktrees.md
modified_files:
  - dev/
  - checks/web-ui/
  - docs/agent/
  - flake.nix
priority: high
type: enhancement
ordinal: 463000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Provide a fast local and agent-facing browser-test loop that runs the existing Web UI integration test implementation against the persistent Crystal Forge development stack. The host runner is for implementation feedback; the existing NixOS Web UI check remains the authoritative verification boundary. Keep the change focused and independent of MR !325 and its shard architecture.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 `web-ui-test [STEP ...]` runs the existing integration-test.js directly against the persistent development stack without invoking the NixOS test harness or downloading browser dependencies.
- [ ] #2 The runner accepts one or more positional workflow names or the existing CF_UI_TEST_STEPS variable and rejects conflicting or invalid input clearly.
- [ ] #3 The runner checks required service readiness and reports useful startup guidance before launching the browser.
- [ ] #4 Repeated runs preserve the development stack and use deterministic browser and test state with predictable ignored artifact output.
- [ ] #5 Workflows that require the NixOS VM fail explicitly rather than being skipped or weakened.
- [ ] #6 Focused wrapper coverage includes argument handling, readiness failures, artifact creation, unsupported workflows when applicable, and browser-runner status propagation.
- [ ] #7 At least one existing host-compatible browser workflow succeeds twice against one real persistent development stack without restarting its services.
- [ ] #8 Documentation distinguishes the fast host development loop from authoritative NixOS verification.
- [ ] #9 The same representative workflow passes through the existing authoritative NixOS Web UI check after the change.
- [ ] #10 Measured evidence compares the old focused NixOS path with first and repeated host-side runs and records derivation, service-restart, and browser-download behavior.
<!-- AC:END -->

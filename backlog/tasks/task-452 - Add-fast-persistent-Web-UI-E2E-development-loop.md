---
id: TASK-452
title: Add fast persistent Web UI E2E development loop
status: In Progress
assignee:
  - openai-agent
created_date: '2026-09-01 21:01'
updated_date: '2026-09-01 21:17'
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

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Add a thin `web-ui-test` shell runner that discovers the current worktree, validates positional or environment-based workflow selection against `coverage-manifest.json`, checks the Dioxus and API endpoints, creates a unique ignored artifact directory, sets the existing harness credentials/origins, and executes `checks/web-ui/tests/integration-test.js` directly.
2. Mark an initial repeat-safe `devStackWorkflows` subset in the existing coverage manifest. Start with `12-systems`, which uses real authentication but mocks its systems responses, does not call VM-only SQL or git services, and does not mutate persistent server state. Reject all unclassified workflows with the authoritative NixOS command.
3. Package the runner through `devScripts` with the same Nix Playwright/browser pairing as the authoritative check and add it directly to the default development shell PATH.
4. Add shell regression coverage for selection conversion, environment selection, conflicts, unknown and VM-only names, readiness failures, artifact creation, and integration-runner exit propagation.
5. Document `run-ui-dev` plus `web-ui-test` as the implementation loop and retain the NixOS Web UI check as the authoritative boundary.
6. Run static/package checks, start one persistent development stack, run `12-systems` twice while measuring service continuity and runtime, then run the same authoritative focused NixOS workflow and applicable flake checks.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implementation worktree: /home/mcamp/code/crystal-forge/TASK-452-fast-web-ui-e2e-loop. Branch: TASK-452-fast-web-ui-e2e-loop. Base: origin/dev at 701151f4237d912ac1065474ec39c0315b86a1ed. Work is intentionally independent of MR !325.

Current origin/dev does not contain the instruction's TASK-440 workflow names; they exist only in other unmerged work. MR !325's shard implementation is not present and will not be cherry-picked. The current persistent stack uses PostgreSQL 3042, API 3445, and Dioxus 8080. The NixOS harness uses Nix-provided playwright-test/playwright-driver. Most current workflows depend on VM SQL, gitserver, evaluation services, ordering, or persistent mutations. `12-systems` is the initial validated repeat-safe host workflow.
<!-- SECTION:NOTES:END -->

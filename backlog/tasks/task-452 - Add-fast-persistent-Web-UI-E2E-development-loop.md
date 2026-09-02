---
id: TASK-452
title: Add fast persistent Web UI E2E development loop
status: Done
assignee:
  - openai-agent
created_date: '2026-09-01 21:01'
updated_date: '2026-09-02 02:41'
labels:
  - web-ui
  - developer-experience
  - e2e
dependencies: []
references:
  - checks/web-ui/tests/integration-test.js
  - checks/web-ui/default.nix
  - dev/packages/devScripts/
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/326'
documentation:
  - docs/agent/verification.md
  - docs/agent/worktrees.md
modified_files:
  - .gitignore
  - .gitlab-ci.yml
  - checks/web-ui-test-runner/default.nix
  - checks/web-ui/coverage-manifest.json
  - checks/web-ui/tests/web-ui-test-runner-test.sh
  - checks/web-ui/tests/web-ui-test.sh
  - docs/agents/verification.md
  - docs/web-ui-check.md
  - packages/devScripts/default.nix
  - shells/default/default.nix
priority: high
type: enhancement
ordinal: 16000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Provide a fast local and agent-facing browser-test loop that runs the existing Web UI integration test implementation against the persistent Crystal Forge development stack. The host runner is for implementation feedback; the existing NixOS Web UI check remains the authoritative verification boundary. Keep the change focused and independent of MR !325 and its shard architecture.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 `web-ui-test [STEP ...]` runs the existing integration-test.js directly against the persistent development stack without invoking the NixOS test harness or downloading browser dependencies.
- [x] #2 The runner accepts one or more positional workflow names or the existing CF_UI_TEST_STEPS variable and rejects conflicting or invalid input clearly.
- [x] #3 The runner checks required service readiness and reports useful startup guidance before launching the browser.
- [x] #4 Repeated runs preserve the development stack and use deterministic browser and test state with predictable ignored artifact output.
- [x] #5 Workflows that require the NixOS VM fail explicitly rather than being skipped or weakened.
- [x] #6 Focused wrapper coverage includes argument handling, readiness failures, artifact creation, unsupported workflows when applicable, and browser-runner status propagation.
- [x] #7 At least one existing host-compatible browser workflow succeeds twice against one real persistent development stack without restarting its services.
- [x] #8 Documentation distinguishes the fast host development loop from authoritative NixOS verification.
- [x] #9 The same representative workflow passes through the existing authoritative NixOS Web UI check after the change.
- [x] #10 Measured evidence compares the old focused NixOS path with first and repeated host-side runs and records derivation, service-restart, and browser-download behavior.
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

Verification on branch head 207ee5119cb48fe2a4dad3c1b4d1744e6bd060f8: web-ui-test-runner check passed; authoritative CF_UI_TEST_STEPS=12-systems web-ui check passed in 1173 s (455.7 s in-VM script); nix flake check --keep-going --no-build passed on the committed tree (104 outputs); shellcheck clean; harness static contracts pass with the extended manifest. Live loop on one persistent stack: first 3.6 s, repeat 3.7 s, two workflows 5.0 s, PostgreSQL/server/Dioxus PIDs unchanged, no derivation built, no browser download.

An earlier local nix flake check failure (path '...-source' is not valid) came from evaluating a dirty worktree copy and from import-from-derivation with --no-build. Both reproduce independently of this change and pass on the clean committed tree.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Adds `web-ui-test`, a thin host-side runner that executes the existing `checks/web-ui/tests/integration-test.js` against the persistent `run-ui-dev` stack, so focused browser iteration no longer builds or boots the NixOS Web UI test environment.

The runner accepts positional workflow names or `CF_UI_TEST_STEPS` and rejects the combination, rejects unknown names, and rejects workflows outside the new `settings.devStackWorkflows` manifest list with the authoritative NixOS command. It checks the Dioxus and Crystal Forge endpoints, then waits for the served application bundle (JS loader and WebAssembly magic header) because the Dioxus server answers HTTP before its first build completes. Artifacts are written to the ignored `.tmp/web-ui-test/<run-id>/`. Browser dependencies come from the same Nix Playwright and Chromium closure the authoritative check uses; nothing is downloaded and no service is restarted.

A new `web-ui-test-runner` check covers selection, conflicts, unknown and VM-only names, readiness failures, artifact creation, and exit-status propagation, and is added to the CI check matrix. Documentation states that `web-ui-test` is the development feedback loop and the NixOS `web-ui` check remains the authoritative boundary.

Measured on this branch with a warm store: focused NixOS check 1173 s versus 3.6 s first and 3.7 s repeated host runs on one persistent stack.

The initial supported list is `12-systems` and `12a-systems-empty-state`. Other workflows still require VM services, direct PostgreSQL fixtures, the test gitserver, or predecessor state, and remain VM-only by design.

MR: https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/326
<!-- SECTION:FINAL_SUMMARY:END -->

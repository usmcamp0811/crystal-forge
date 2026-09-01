---
id: TASK-438
title: Make Web UI Nix check fail when integration manifest steps fail
status: In Progress
assignee:
  - opencode-gpt-5.6-sol
created_date: '2026-08-26 01:33'
updated_date: '2026-09-01 03:59'
labels:
  - web-ui
  - testing
  - nix
dependencies: []
references:
  - TASK-433.5
  - TASK-450.11
modified_files:
  - checks/web-ui/default.nix
  - checks/web-ui/tests/browser-verdict.js
  - checks/web-ui/tests/browser-verdict.test.js
  - checks/web-ui/tests/integration-test.js
priority: high
type: bug
ordinal: 447000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The authoritative `checks.x86_64-linux.web-ui` derivation can exit successfully even when `integration-test.js` reports failed manifest steps. During TASK-433.5 verification, `nix flake check --keep-going -L` reported all checks passed while the Web UI report captured 63/104 screenshots and listed numerous failed steps. This makes derivation success insufficient proof and can mask regressions.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The Web UI Nix check exits non-zero when any required selected integration step fails
- [ ] #2 The check preserves and reports the failed step names and reasons
- [ ] #3 A regression test proves a deliberately failing selected step causes the derivation to fail
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Define one machine-readable browser result contract that lists every selected step, failed step name, failure reason, process completion, and available artifacts.
2. Make `integration-test.js` return non-zero after it finishes writing reports whenever any selected required step fails. Make the Nix driver wait for the process exit marker instead of treating early `results.json` creation as completion.
3. Separate report production from the blocking gate so a failed logical gate retains an addressable evidence derivation. Expose the evidence through the blocking check for CI recovery.
4. Add Node regression tests for all-passing, failed-step, missing-result, and process-exit outcomes. Verify focused and named-group execution use the same verdict path.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
TASK-450.11 treats this bug as a correctness prerequisite for web UI latency work. Required browser-step failures must propagate before the check is split, parallelized, or given tighter timing bounds; otherwise faster completion could preserve a false-success result. Preserve failed step names, reasons, and artifacts in a form that remains usable if the harness is later divided into multiple checks.

The user selected this task for the focused Web UI latency bundle and explicitly requested one shared branch, worktree, and MR with TASK-354 and TASK-450.11.1 through TASK-450.11.3.

LOCK: opencode-gpt-5.6-sol in /home/mcamp/code/crystal-forge/TASK-450-web-ui-parallel-checks on branch TASK-450.11-web-ui-parallel-checks, based on TASK-450-p0-build-graph at 437efd55.

Implemented the JavaScript/harness reliability portion in the shared TASK-450 worktree. Added a versioned selected-step verdict contract with Node built-in regression tests, wrote results and verdict before assigning the process exit code, made unhandled rejections process-fatal without suppressing report production, and changed the Nix driver to wait for `integration.exit` and report every selected failed step. Verification passed in `nix develop`: Node syntax checks for all changed scripts, 3/3 `node:test` cases, `nix-instantiate --parse checks/web-ui/default.nix`, and `git diff --check`. Full VM execution was not run in this focused harness pass.
<!-- SECTION:NOTES:END -->

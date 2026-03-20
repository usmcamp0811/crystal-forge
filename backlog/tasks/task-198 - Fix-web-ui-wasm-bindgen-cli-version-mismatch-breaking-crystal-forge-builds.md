---
id: TASK-198
title: Fix web-ui wasm-bindgen-cli version mismatch breaking crystal-forge builds
status: Review
assignee:
  - OpenCode
created_date: '2026-03-20 02:58'
updated_date: '2026-03-20 03:26'
labels: []
dependencies: []
references:
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/issues/115'
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Problem: Builds that include crystal-forge currently fail in both dev and main because crystal-forge-web-ui forces Dioxus to use a wasm-bindgen-cli path labeled 0.2.108 while nixpkgs provides 0.2.114, causing dx bundle to abort.

Goal: Restore successful crystal-forge-web-ui builds by eliminating the incompatible wasm-bindgen override in the web-ui derivation while keeping existing build behavior intact.

Non-Goals:
- Upgrading all frontend/toolchain dependencies across the repository
- Refactoring unrelated web-ui build steps
- Changing runtime UI behavior

Architectural Constraints:
- Keep fix scoped to Nix packaging/build configuration for web-ui
- Prefer alignment with Dioxus toolchain expectations over custom version forcing

Verification Plan:
- Run a targeted Nix build for crystal-forge-web-ui in nix develop environment
- Run a follow-up build for crystal-forge package path that previously failed
- Confirm no wasm-bindgen-cli mismatch error appears

Impact Areas:
- Nix packaging for crystal-forge-web-ui
- Build pipeline for crystal-forge on dev/main

Risk Level: Medium (build-tooling change with broad build impact)

Source issue: GitLab issue #115
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Building crystal-forge-web-ui no longer fails with wasm-bindgen-cli version mismatch error
- [x] #2 The web-ui derivation no longer forces a mismatched wasm-bindgen binary path for Dioxus
- [x] #3 A targeted build path that previously failed now completes successfully in local verification
- [x] #4 Changes are limited to files required for this build break fix
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Locate the web-ui Nix derivation logic that injects a wasm-bindgen path into Dioxus build tooling.
2. Remove or adjust the forced wasm-bindgen symlink so the build uses a compatible wasm-bindgen-cli version.
3. Run targeted Nix builds to verify crystal-forge-web-ui and affected crystal-forge build paths succeed without version mismatch error.
4. Update task notes and acceptance criteria checkboxes with verification evidence.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Moved to To Do per explicit user request to address production-breaking issue ASAP (GitLab #115).

LOCK: OpenCode on reckless in /home/mcamp/code/crystal-forge/TASK-198-fix-wasm-bindgen-mismatch

Implemented hotfix in `packages/web-ui/default.nix`: replaced `pkgs.wasm-bindgen-cli` with `pkgs.wasm-bindgen-cli_0_2_108` for web build, desktop build, and dx-serve runtime wiring.

Verification run in task worktree `/home/mcamp/code/crystal-forge/TASK-198-fix-wasm-bindgen-mismatch`: `nix develop -c nix build .#web-ui`, `nix develop -c nix build .#default`, and `nix develop -c nix flake check` all completed successfully.

MR (dev): https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/173

MR (main): https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/174

Created main-based cherry-pick branch `TASK-198-main-cherry-pick` from `main` and cherry-picked commit `f911d6e7` to keep main MR diff scoped to the hotfix only.

Opened replacement main MR: https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/175

Closed superseded main MR: https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/174
<!-- SECTION:NOTES:END -->

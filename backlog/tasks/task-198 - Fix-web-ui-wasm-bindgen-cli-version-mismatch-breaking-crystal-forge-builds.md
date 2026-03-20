---
id: TASK-198
title: Fix web-ui wasm-bindgen-cli version mismatch breaking crystal-forge builds
status: To Do
assignee: []
created_date: '2026-03-20 02:58'
updated_date: '2026-03-20 02:58'
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
- [ ] #1 Building crystal-forge-web-ui no longer fails with wasm-bindgen-cli version mismatch error
- [ ] #2 The web-ui derivation no longer forces a mismatched wasm-bindgen binary path for Dioxus
- [ ] #3 A targeted build path that previously failed now completes successfully in local verification
- [ ] #4 Changes are limited to files required for this build break fix
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Moved to To Do per explicit user request to address production-breaking issue ASAP (GitLab #115).
<!-- SECTION:NOTES:END -->

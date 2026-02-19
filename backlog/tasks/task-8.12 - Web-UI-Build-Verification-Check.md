---
id: TASK-8.12
title: Web UI Build Verification Check
status: Done
assignee: []
created_date: '2026-02-12 10:00'
updated_date: '2026-02-19 01:51'
labels:
  - ui
  - nix
  - testing
  - ci
milestone: m-3
dependencies:
  - TASK-8.1
  - TASK-8.9
parent_task_id: TASK-8
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Add a Nix flake check that verifies the web UI builds correctly. This was prompted by a blank-page incident where we had no automated way to verify the web UI output was valid.

The check should:
1. Run `dx build` on the web-ui source in a sandboxed Nix derivation
2. Verify `index.html` exists in the build output
3. Verify a `.wasm` binary exists in the output
4. Verify `index.html` references the WASM loader script

This provides fast CI-level confidence that the web UI compiles and produces valid output, without needing a browser or running server.

Implementation:
- Create `checks/web-ui/default.nix` using `pkgs.runCommand` (not a VM test — too heavy for a build check)
- Reuse the same toolchain from `packages/web-ui/default.nix` (dioxus-cli, rustc, cargo, wasm-bindgen-cli, binaryen)
- Run `dx build` in a writable copy of the source, then validate the output directory

Expected: `nix flake check` (or `nix build .#checks.x86_64-linux.web-ui`) passes when web-ui source is valid.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 #1 `checks/web-ui/default.nix` exists and is discovered by Snowfall Lib
- [ ] #2 #2 Check runs `dx build` successfully in a sandboxed derivation
- [ ] #3 #3 Check verifies `index.html` exists in build output
- [ ] #4 #4 Check verifies `.wasm` binary exists in build output
- [ ] #5 #5 Check verifies `index.html` references the WASM loader
- [ ] #6 #6 `nix build .#checks.x86_64-linux.web-ui` passes

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
<!-- AC:END -->

Marked Done as OBE/fixed: web UI build verification check has already been implemented; task closed per maintainer direction.
<!-- SECTION:NOTES:END -->

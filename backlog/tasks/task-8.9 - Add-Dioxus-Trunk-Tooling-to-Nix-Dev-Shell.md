---
id: TASK-8.9
title: Add Dioxus/Trunk Tooling to Nix Dev Shell
status: Done
assignee:
  - KimiK2.5
created_date: '2026-02-11 10:00'
updated_date: '2026-03-13 01:24'
labels:
  - ui
  - nix
  - tooling
milestone: m-3
dependencies: []
parent_task_id: TASK-8
priority: high
ordinal: 51000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Add trunk, wasm-bindgen-cli, wasm-opt, and wasm32-unknown-unknown target to the Nix development shell so Dioxus web builds work out of the box.

This is the true first task for UI development - it unblocks all other UI work.

Steps:
1. Add trunk to flake.nix devShell packages
2. Add wasm-bindgen-cli for WASM binding generation
3. Add wasm-opt (from binaryen) for WASM optimization
4. Ensure wasm32-unknown-unknown Rust target is available (via Nix rust overlay or rustup)
5. Verify: nix develop -c bash -c "trunk --version && wasm-bindgen --version"
6. Test a minimal trunk build to confirm the toolchain works end-to-end
7. Document any Nix-specific gotchas (e.g., OpenSSL for reqwest WASM, cargo target dir)

Architecture notes:
- The existing flake.nix uses snowfall-lib, so dev shell config may be in shells/ or similar
- Trunk needs to find wasm-bindgen-cli on PATH
- Consider whether to use rust-overlay for wasm target or nixpkgs rust with targets

Expected: Running `nix develop` gives you everything needed to `trunk serve` a Dioxus web app
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 #1 #1 #1 trunk available in nix develop shell
- [x] #2 #2 #2 #2 wasm-bindgen-cli available in nix develop shell (v0.2.108)
- [x] #3 #3 #3 #3 wasm-opt (binaryen) available in nix develop shell
- [x] #4 #4 #4 #4 Minimal trunk/dx build succeeds in nix develop (cargo wasm32 compilation verified)
- [x] #5 #5 #5 #5 No manual rustup/cargo-install steps required

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
- Bumped nixpkgs from release-25.05 to release-25.11 for Dioxus 0.7.3 support
- Added `dioxus-cli` (dx 0.7.3), `trunk`, `wasm-bindgen-cli`, and `binaryen` to `shells/default/default.nix` buildInputs
- Added UI development section to shell welcome message
- nixpkgs rustc (1.91.1) includes wasm32-unknown-unknown std lib - no rust-overlay needed
- **IMPORTANT**: wasm-bindgen-cli is v0.2.108 - the wasm-bindgen crate in Cargo.toml must match
- Verified: dx 0.7.3, rustc 1.91.1, wasm32 target functional
<!-- AC:END -->
<!-- SECTION:NOTES:END -->

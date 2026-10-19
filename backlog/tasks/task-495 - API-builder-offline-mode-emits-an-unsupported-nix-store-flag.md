---
id: TASK-495
title: API builder offline mode emits an unsupported nix-store flag
status: In Progress
assignee:
  - '@openai'
created_date: '2026-10-19 05:37'
updated_date: '2026-10-19 13:08'
labels: []
dependencies: []
references:
  - packages/default/crates/cf-builder/src/derivations/build.rs
  - modules/nixos/crystal-forge/default.nix
  - checks/integration-delivery/scenarios/fragments/source-build.py
  - /tmp/opencode/delivery-local-run12.log
priority: high
type: bug
ordinal: 546000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
A real API-only builder received and materialized an authorized NixOS derivation, then failed before realization because its configured offline mode passed --offline to nix-store 2.34.8. The selected CLI rejects that flag. Observed during the Delivery lifecycle on the TASK-490.1 branch based on e07b4bd2, with services.crystal-forge.build.offline=true. The captured build log states: error: unknown flag '--offline'. Production changes are prohibited in the testing overhaul, so this record tracks the product defect separately. This is not the already-resolved state-machine CI networking or Bash issue.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A configured offline API-only builder can realize an authorized derivation using the supported Nix CLI without an unsupported-flag error.
- [ ] #2 Offline realization cannot acquire external substitutes or use remote builders or network fallback.
- [ ] #3 Tests cover the effective invocation and a real successful local realization with its output absent before execution.
- [ ] #4 Affected configuration and maintainer documentation describe the actual offline guarantee and missing-prerequisite failure.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Owner-authorized separate product correction. Worktree /home/mcamp/code/crystal-forge/TASK-495-builder-offline, branch TASK-495-builder-offline, base dev at f3db8618. No main worktree is checked out; dev has no tracked modifications and its unrelated locks/session file remain untouched. Task lock is active.
1. Correct the shared BuildConfig::apply_to_command offline branch in cf-config, which currently appends modern-only --offline even when the caller is nix-store --realise. Use legacy-compatible Nix options that explicitly disable substitution, external substituters, remote builders and fallback. Keep ordinary online/resource/sandbox behavior unchanged and preserve all actual build assertions.
2. Add focused ordinary cf-config argument tests for direct nix-store, systemd-scoped nix-store and modern Nix callers, including offline overriding use_substitutes=true and online behavior unchanged. No new gate or testing framework.
3. Update adjacent public Rust contracts and the existing NixOS build.offline option documentation. Distinguish Nix input-acquisition policy from an OS network firewall and from API/publication traffic.
4. Run targeted cf-config tests, formatting and rustdoc through Nix, then build the real builder package. Commit the bounded product fix separately. Apply that reviewed product commit to the coordinator branch without modifying its Delivery assertions/configuration. Run the same offline=true Delivery proof to establish actual absent-before/present-after Nix realization; retain precise failures and no xfail. Do not merge/deploy or mark Done.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
The owner explicitly authorized a separate product fix in response to the Delivery blocker question. The test must keep offline=true and retain all build, output and network assertions. Use a dedicated task worktree; do not treat the failure as expected success.
<!-- SECTION:NOTES:END -->

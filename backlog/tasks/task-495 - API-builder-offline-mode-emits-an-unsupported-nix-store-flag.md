---
id: TASK-495
title: API builder offline mode emits an unsupported nix-store flag
status: To Do
assignee: []
created_date: '2026-10-19 05:37'
updated_date: '2026-10-19 13:06'
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

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
The owner explicitly authorized a separate product fix in response to the Delivery blocker question. The test must keep offline=true and retain all build, output and network assertions. Use a dedicated task worktree; do not treat the failure as expected success.
<!-- SECTION:NOTES:END -->

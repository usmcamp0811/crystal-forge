---
id: TASK-287
title: Clean build system names by extracting from flake attribute path
status: To Do
assignee: []
created_date: '2026-05-04 00:25'
labels:
  - ui
  - web-ui
  - builds
  - polish
milestone: UI/UX Design System
dependencies: []
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem
Build UI currently shows full flake attribute paths like "nixosConfigurations.daly" or "nixosConfigurations.test.gray" instead of clean system names like "daly" or "gray".

## Goal
Update pkg() and drv() methods to use extract_system_name() helper to show clean, human-readable system names throughout the Builds UI.

## Non-Goals
- No API changes
- No changes to how data is stored or transmitted
- Only affects display logic in the web-ui package

## Current State (TASK-283 implementation)
```rust
pub fn pkg(&self) -> &str {
    &self.hostname  // Returns "nixosConfigurations.daly"
}

pub fn drv(&self) -> String {
    format!("/nix/store/{}xxxx-nixos-system-{}.drv", commit_prefix, self.hostname)
    // Uses full hostname in path
}
```

## Desired State
```rust
pub fn pkg(&self) -> String {
    extract_system_name(&self.hostname).to_string()  // Returns "daly"
}

pub fn drv(&self) -> String {
    let clean_name = extract_system_name(&self.hostname);
    format!("/nix/store/{}xxxx-nixos-system-{}.drv", commit_prefix, clean_name)
}
```
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Build queue table shows clean system names like 'daly' instead of 'nixosConfigurations.daly'
- [ ] #2 Build detail panel shows clean system name in title
- [ ] #3 Build log modal shows clean system name in header
- [ ] #4 Derivation paths use clean system names in the synthesized store path
- [ ] #5 extract_system_name() helper is used consistently for all build system name displays
- [ ] #6 cargo check passes for web-ui package
- [ ] #7 No visual regressions in build queue, detail panel, or log modal
<!-- AC:END -->

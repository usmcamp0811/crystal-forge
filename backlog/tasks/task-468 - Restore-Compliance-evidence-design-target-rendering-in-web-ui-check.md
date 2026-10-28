---
id: TASK-468
title: Restore Compliance evidence design-target rendering in web-ui check
status: Backlog
assignee: []
created_date: '2026-09-30 01:24'
labels: []
dependencies: []
references:
  - checks/web-ui/design-parity/generate-design-targets.js
  - checks/web-ui/design-parity/generate-design-targets-test.js
  - TASK-440
priority: high
type: bug
ordinal: 489000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The authoritative Web UI check cannot start its NixOS browser runner because the design-target generation dependency fails to render the Compliance evidence examples. On TASK-326.2.2, `CF_UI_TEST_STEPS=16-cves,16e-poam-register-design nix build --impure .#checks.x86_64-linux.web-ui --no-link -L` produced 46/54 design targets: both `compliance-evidence` themes timed out waiting for a visible Compliance navigation item. TASK-440 separately owns four configuration design-target failures in the same build. This is an unrelated check dependency, not evidence that the CVE or POA&M workflows failed.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The Compliance evidence dark and light design targets render from the current design application without timeout.
- [ ] #2 The design-target generator reports all expected Compliance evidence targets and preserves existing CVE and POA&M target output.
- [ ] #3 A focused regression check demonstrates the generator completes on a clean worktree with a rendered Compliance evidence target.
- [ ] #4 Source documentation or test fixture expectations that describe the Compliance navigation are updated to match the intended design.
<!-- AC:END -->

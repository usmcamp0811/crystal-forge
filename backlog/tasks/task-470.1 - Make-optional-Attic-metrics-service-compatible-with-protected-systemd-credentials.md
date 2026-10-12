---
id: TASK-470.1
title: >-
  Make optional Attic metrics service compatible with protected systemd
  credentials
status: Backlog
assignee: []
created_date: '2026-10-12 01:02'
labels: []
dependencies: []
references:
  - modules/nixos/attic-metrics/default.nix
  - packages/attic-metrics/attic_metrics.py
  - checks/attic-metrics/README.md
parent_task_id: TASK-470
priority: high
type: bug
ordinal: 504000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The TASK-470 native Attic inventory fixture reproduced optional-module startup failure on the locked systemd 260.2 environment. Under the module sandbox both LoadCredential copies were regular and correctly owned but had group/other permission bits; cf-attic-metrics rejected them and never opened its listener. The focused API gate uses the user-authorized standalone invocation with service-owned mode-0600 files. Operators need the optional module to start without weakening credential protection. The user prohibited provider/module edits in the delegated check work.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The optional module starts the packaged adapter with protected runtime credentials on the locked systemd environment
- [ ] #2 Credential ownership and isolation remain enforced and unsafe public files or symlinks remain rejected
- [ ] #3 An isolated native VM regression exercises optional-module startup and authenticated inventory without printing secrets
- [ ] #4 Operator documentation accurately describes the supported credential delivery and protection contract
<!-- AC:END -->

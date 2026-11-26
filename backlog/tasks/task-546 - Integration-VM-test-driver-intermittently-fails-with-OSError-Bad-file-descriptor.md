---
id: TASK-546
title: >-
  Integration VM test driver intermittently fails with OSError Bad file
  descriptor
status: Backlog
assignee: []
created_date: '2026-11-26T17:10:31Z'
labels:
  - testing
dependencies: []
priority: medium
type: bug
ordinal: 580000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem
Delivery integration scenarios intermittently fail inside the NixOS test driver with `OSError: [Errno 9] Bad file descriptor`, after which diagnostic collection on every node fails with the same error.

Observed:
- Pipeline 3100779917 (MR !353), job integration-delivery, scenario failure-reporting, case failure-projection-real-role-identities-and-independent-telemetry, after 560.9 s, VM run local to the runner.
- Pipeline 3119599541 (MR !356), job integration-delivery/delivery-lifecycle, case publication-rejects-untrusted-signature-then-accepts-same-signed-output, after 1549.0 s, VM run on a build-farm builder.

The run derivation fails, so its evidence output is not produced.

## Desired outcome
Identify whether the driver loses a VM console/socket (resource pressure, timeout, guest crash) and make the scenarios fail with a cause, or not fail, without weakening assertions.
<!-- SECTION:DESCRIPTION:END -->

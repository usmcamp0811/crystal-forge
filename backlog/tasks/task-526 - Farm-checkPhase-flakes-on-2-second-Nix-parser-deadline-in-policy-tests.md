---
id: TASK-526
title: Farm checkPhase flakes on 2-second Nix parser deadline in policy tests
status: Backlog
assignee: []
created_date: '2026-11-15T21:19:00Z'
labels:
  - server
  - ci
  - tests
dependencies: []
priority: medium
type: bug
ordinal: 574000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

On the remote build farm, the `cf-server` and `cf-server-core` checkPhase intermittently fails 15-17 unit tests in `models::deployment_policies`, `handlers::api::deployment_policies`, `handlers::api::compliance`, and `queries::compliance_interchange`. Each failure reports "Nix syntax validation unavailable: the Nix parser did not finish within 2 seconds" (`handlers/api/deployment_policies.rs`). Retrying the same derivation sometimes passes (TASK-521 notes). TASK-525.1 saw the failure on two consecutive farm builds on 2026-11-15.

## Desired outcome

Unit tests that check parser results do not depend on a wall-clock deadline that a loaded builder can exceed. The production 2-second bound and its 503 behavior stay unchanged.
<!-- SECTION:DESCRIPTION:END -->

---
id: TASK-548
title: 'CI: remaining compilation sharing between cf-server core and embedded builds'
status: Backlog
assignee: []
created_date: '2026-11-27T00:06:11Z'
labels:
  - ci
dependencies: []
priority: low
type: chore
ordinal: 582000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem
After TASK-542.1, cf-server-core-drv (tests) and cf-server-drv (production, embedded-ui) still compile cf-protocol, cf-config and cf-compliance separately; cf-server itself differs by feature and must compile twice. Chaining cf-server-drv on core's Crane artifacts would make every integration scenario wait for core's full test phase, so it was not done.

## Desired outcome
Measure the cost of the shared crates on a cold build and decide whether a shared Crane workspace-artifact stage (or similar native mechanism) is worth it without serializing the embedded build behind core tests.
<!-- SECTION:DESCRIPTION:END -->

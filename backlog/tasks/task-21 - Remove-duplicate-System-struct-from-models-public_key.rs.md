---
id: TASK-21
title: Remove duplicate System struct from models/public_key.rs
status: Backlog
assignee: []
created_date: '2026-02-13 04:25'
updated_date: '2026-02-19 03:39'
labels:
  - refactoring
  - tech-debt
dependencies: []
priority: medium
milestone: m-2
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
There are two System structs: models/systems.rs (canonical, has desired_target + deployment_policy) and models/public_key.rs (stale duplicate, missing those fields). The duplicate should be removed and all usages updated to use the canonical one. Discovered while working on TASK-2.1.
<!-- SECTION:DESCRIPTION:END -->

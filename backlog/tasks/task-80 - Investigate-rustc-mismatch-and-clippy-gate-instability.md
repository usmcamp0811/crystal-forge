---
id: TASK-80
title: Investigate rustc mismatch and clippy gate instability
status: Backlog
assignee: []
created_date: '2026-02-22 04:27'
labels: []
dependencies: []
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Clippy with -D warnings fails in devshell due to rustc artifact mismatch (E0514: 1.91.1 vs 1.92.0) and a large set of existing workspace lint violations unrelated to TASK-65.7. Define a deterministic lint workflow and baseline for CI/local.
<!-- SECTION:DESCRIPTION:END -->

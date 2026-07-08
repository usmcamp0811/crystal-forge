---
id: TASK-80
title: Investigate rustc mismatch and clippy gate instability
status: Backlog
assignee: []
created_date: '2026-02-22 04:27'
updated_date: '2026-07-08 13:29'
labels: []
dependencies: []
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Clippy with -D warnings fails in devshell due to rustc artifact mismatch (E0514: 1.91.1 vs 1.92.0) and a large set of existing workspace lint violations unrelated to TASK-65.7. Define a deterministic lint workflow and baseline for CI/local.
<!-- SECTION:DESCRIPTION:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: gpt-5.5
created: 2026-07-08 13:29
---
Observed during TASK-384 verification: `cargo clippy --all-targets -- -D warnings` fails in both `packages/default` and `packages/web-ui` due to broad pre-existing warnings (unused imports/dead code/style/deprecated Dioxus key warnings, etc.). The TASK-384 branch fixed the small task-touched unused imports/mutability warnings it introduced, but the workspace baseline still blocks using `-D warnings` as a completion gate.
---
<!-- COMMENTS:END -->

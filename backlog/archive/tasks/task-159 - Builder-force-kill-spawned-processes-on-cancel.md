---
id: TASK-159
title: Builder force kill spawned processes on cancel
status: Backlog
assignee: []
created_date: '2026-03-02 16:16'
labels: []
dependencies: []
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
When a build job is cancelled (either manually or due to shutdown), the builder should immediately terminate any spawned processes (like `systemd-run --user -E HOME`) instead of waiting for graceful termination which can take a long time.

## Problem

Currently, when process-compose is closed or a build job is cancelled, the builder waits for spawned processes to terminate gracefully. This can take a very long time because:
1. The builder spawns `systemd-run --user -E HOME -P nix copy ...` to transfer build outputs
2. When cancelled, the process-compose tries to cleanly stop these processes
3. The systemd-run process may not terminate quickly, blocking shutdown

## Desired Outcome

The builder should implement a force-kill mechanism that:
1. Detects when a build job is cancelled or the system is shutting down
2. Immediately terminates spawned child processes (using process groups, SIGKILL, etc.)
3. Avoids blocking the shutdown sequence

## Impact Areas

- Builder process management
- Process lifecycle handling
- process-compose integration
<!-- SECTION:DESCRIPTION:END -->

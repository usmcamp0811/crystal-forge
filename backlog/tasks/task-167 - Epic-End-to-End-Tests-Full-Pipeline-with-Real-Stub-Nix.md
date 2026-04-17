---
id: TASK-167
title: 'Epic: End-to-End Tests - Full Pipeline with Real/Stub Nix'
status: Backlog
assignee: []
created_date: '2026-03-04 03:09'
labels: []
milestone: m-15
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem
End-to-end tests needed to prove the entire pipeline actually works with real (or stub) Nix.

## Goals
- Happy-path end-to-end: Commit → eval → build → cache → agent deploy
- Canary / per-host desired_target with different deployment policies
- Partial rollout correctness (new commit builds only for subset)

## Scope
Real Nix where possible, or mini Nix stub. Full pipeline integration.

## Release Blockers
- Agent convergence

## Acceptance Criteria
- [ ] Commit → eval → build → cache → agent deploy (tiny flake, 1-2 nixosConfigurations)
- [ ] Final state: cache has store path, host desired_target updated, agent switched, system_state record created
- [ ] Two hosts, different deployment policies (auto_latest vs pinned/manual)
- [ ] DPM updates auto_latest host to newest; pinned unchanged
- [ ] New commit builds only for subset (system X passes, system Y fails eval policy)
- [ ] DPM updates desired_target only for passing systems
<!-- SECTION:DESCRIPTION:END -->

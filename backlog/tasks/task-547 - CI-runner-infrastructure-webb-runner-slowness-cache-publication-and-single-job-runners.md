---
id: TASK-547
title: >-
  CI runner infrastructure: webb runner slowness, cache publication and
  single-job runners
status: Backlog
assignee: []
created_date: '2026-11-27T00:05:53Z'
labels:
  - ci
dependencies: []
priority: medium
type: chore
ordinal: 581000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem
Measured during MR !356 (TASK-542.1). Runner infrastructure is managed in the Campground/dotfiles repositories, not here.

- webb (GitLab runner 59818941) ran the state-machine database suite about 10x slower than sledge, lucas and reckless: access_tokens run ~29 min vs ~2 min; migration phase 104-107 s vs 6 s. Both webb runs exceeded the 2-hour job limit (pipelines 3120173363, 3120511772); runs elsewhere take 31-53 min. webb is also a nix-grpc-store farm builder.
- webb has no niks3-auto-upload and is not a farm client, so packages it builds are not shared with other runners (pipeline 3119599541: integration-pipeline built cf-server on webb, the Access scenario rebuilt it).
- The four project runners set no gitlab-runner `concurrent` value (default 1), so the project has four job slots; scenario jobs queued 7-22 min.

## Desired outcome
Decide in the infrastructure repositories: fix or exclude webb for heavy jobs, enable cache publication on webb, and size runner concurrency against host resources (VMs, Cargo, Nix builds).
<!-- SECTION:DESCRIPTION:END -->

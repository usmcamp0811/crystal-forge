---
id: TASK-541
title: Intermittent cf-agent test short_basic_credentials_are_redacted_before_marking_report_safe
status: Backlog
assignee: []
created_date: '2026-11-22'
labels: [testing, agent]
dependencies: []
references:
  - TASK-529
  - TASK-540
priority: medium
---

## Description
<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

The `cf-agent` unit test
`deployment::agent::niks3_tests::short_basic_credentials_are_redacted_before_marking_report_safe`
(`packages/default/crates/cf-agent/src/deployment/agent.rs`, added by
TASK-529 / !344) failed once on the build farm and passed on a retry of the
same source.

Observed on 2026-11-22 while building `.#packages.x86_64-linux.agent` for
TASK-540 (versions bumped to 0.4.0, no change to `cf-agent` source):

- First build: the test panicked at `agent.rs:2569:49` with
  `called Option::unwrap() on a None value`. That line is
  `deployment_failure_display(&error).unwrap()`. The package build failed
  (`cf-agent` 44 passed, 1 failed).
- Retry of the same working tree: `cf-agent` 45 passed, 0 failed.
- `origin/dev` at `dd5ff524` built without a test failure.

The cause is not established. The test writes and executes a temporary
`#!/bin/sh` fixture, and the retry passes. Check whether a race in that
fixture (for example executing a script that is still open for writing) can
make the copy fail with a different error, so that no display text is
produced. This is a hypothesis, not a finding.

## Desired outcome

The test passes on every run, or a documented defect in
`deployment_failure_display` is fixed.

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The cause of the `None` result is identified with evidence (a log or a reproduction), not assumed.
- [ ] #2 The test or the code it covers is fixed, and the fix is run repeatedly on the farm without a failure.
<!-- AC:END -->
<!-- SECTION:DESCRIPTION:END -->

---
id: TASK-136
title: Split nix flake check into per-check GitLab CI jobs
status: Done
assignee: []
created_date: '2026-02-27 02:31'
updated_date: '2026-03-13 01:24'
labels: []
dependencies: []
priority: high
ordinal: 78000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
### Problem Statement

The current GitLab pipeline runs `nix flake check` as a single CI job, making it hard to see progress and isolate failing checks quickly.

### Goal

Run flake checks as separate GitLab CI jobs so each check has explicit visibility and independent pass/fail status.

### Non-Goals

- Do not redesign unrelated parts of the CI pipeline.
- Do not change check semantics beyond splitting execution into per-check jobs.
- Do not modify application runtime behavior.

### Architectural Constraints

- Keep changes scoped to `.gitlab-ci.yml` and task tracking.
- Preserve existing pipeline stage usage and trigger conditions for flake checks.
- Keep runner and token setup behavior consistent with existing check jobs.

### Verification Plan

- `nix flake check --show-trace --no-build .#checks.x86_64-linux.server`
- `git status --short`

### Impact Areas

- GitLab CI pipeline visibility and diagnostics

### Risk Level

Medium: CI structure changes can block merges if selectors or rules are incorrect.

### Dependencies

- Existing flake checks under `checks.x86_64-linux.*`
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 `.gitlab-ci.yml` no longer uses a single monolithic flake-check execution path.
- [ ] #2 Flake checks run as separate CI jobs with per-check job names in GitLab.
- [ ] #3 Per-check flake jobs run for `merge_requests` and `main`.
- [ ] #4 Each job executes exactly one check output from `checks.x86_64-linux`.
- [ ] #5 Verification evidence is recorded in task notes.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Replace single `flake-check` job script with a matrix-driven per-check job.
2. Keep existing runner tags, auth setup, stage, and pipeline rules.
3. Validate a representative check selector locally and capture evidence.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
- LOCK: opencode on gray in ~/code/crystal-forge/TASK-136-split-flake-checks-ci
- Verification evidence:
  - `nix flake check --show-trace --no-build .#checks.x86_64-linux.server` failed immediately because `nix flake check` does not accept a check fragment selector in this form.
  - `nix build .#checks.x86_64-linux.server --dry-run -L --show-trace` was attempted but exceeded local execution timeout.
  - `nix flake show --all-systems` completed and confirmed all check names used by CI matrix (`attic_cache`, `builder`, `dashboard`, `database`, `oidc-auth`, `s3_cache`, `server`, `web-ui`).
- MR: https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/141
<!-- SECTION:NOTES:END -->

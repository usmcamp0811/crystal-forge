---
id: TASK-135
title: Enable Codex code review in GitLab CI
status: In Progress
assignee: []
created_date: '2026-02-27 01:08'
updated_date: '2026-02-27 02:00'
labels: []
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
<!-- SECTION:DESCRIPTION:BEGIN -->
### Problem Statement

Current GitLab CI configuration does not support running Codex code review in merge request pipelines.

### Goal

Update `.gitlab-ci.yml` so Codex code review can run successfully in merge request pipelines with explicit rules and safe variable handling.

### Non-Goals

- Do not redesign the full CI pipeline.
- Do not change application runtime behavior.
- Do not refactor unrelated CI jobs.

### Architectural Constraints

- Keep CI changes scoped to `.gitlab-ci.yml` and task notes.
- Reference sensitive values via CI variables only; no hardcoded tokens.
- Preserve existing stage ordering and existing job trigger behavior unless required by Codex review integration.

### Impact Areas

- Infrastructure (GitLab CI)
- Merge request review workflow

### Risk Level

Medium - a CI rules mistake can create merge-blocking or noisy pipelines.

### Dependencies

- GitLab runner with internet access.
- CI variables configured for Codex execution (`OPENAI_API_KEY`, optional model variable).
- No backlog dependency blockers.

Desired Outcome: `.gitlab-ci.yml` supports Codex code review in MR pipelines under explicit rules.

<!--
SECTION:DESCRIPTION:END
-->
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 `.gitlab-ci.yml` defines a Codex code review job.
- [x] #2 The Codex job runs only for merge request pipelines by explicit rules.
- [x] #3 The Codex job references API credentials through CI variables only (no committed secrets).
- [x] #4 Existing non-Codex jobs keep their prior pipeline contexts.
- [x] #5 CI syntax validates and the new job appears in MR pipeline graph/rules evaluation.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Inspect current CI stages and merge-request-only job patterns.
2. Add a dedicated Codex review stage/job with strict MR rules and variable-based auth.
3. Lint CI YAML and review diff to ensure existing jobs are unaffected.
4. Record verification notes in task notes.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
LOCK: codex-gpt-5.3 on gray in /home/mcamp/code/crystal-forge/TASK-135-enable-codex-codereview-ci

Verification (local): nix develop -c nix run nixpkgs#glab -- ci lint --include-jobs => CI/CD YAML is valid.\nImplemented: codex-code-review job in ai_review stage with MR-only rules and CI-variable-based auth (OPENAI_API_KEY).

Verification (full): nix develop -c nix flake check => passed (warnings only about unknown flake outputs/incompatible systems).
<!-- SECTION:NOTES:END -->

## Verification Plan

Automated:

- `nix develop -c nix run nixpkgs#yamllint -- .gitlab-ci.yml`

Manual:

- Validate job rules in MR pipeline context and confirm Codex job inclusion.
- Confirm no secrets are hardcoded in repository files.

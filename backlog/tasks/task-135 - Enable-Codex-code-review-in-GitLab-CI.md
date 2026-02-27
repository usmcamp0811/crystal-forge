---
id: TASK-135
title: Enable Codex code review in GitLab CI
status: To Do
assignee: []
created_date: "2026-02-27 01:08"
updated_date: "2026-02-27 01:14"
labels: []
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->

### Problem Statement

Current GitLab CI configuration does not support running Codex code review in merge request pipelines.

### Goal

Update `.gitlab-ci.yml` so Codex code review can run successfully in merge request pipelines with safe execution rules and clear pipeline behavior.

### Non-Goals

- Do not redesign the full CI pipeline.
- Do not add unrelated CI optimization/refactors.
- Do not change application runtime behavior.

### Architectural Constraints

- Keep CI changes scoped to GitLab pipeline configuration and job wiring.
- Preserve existing stage ordering and required checks unless explicitly needed for Codex integration.
- Avoid introducing secrets in repository files; use CI variables only.
- Keep deterministic behavior for MR pipelines (no uncontrolled/manual-only paths for required review flow).

### Impact Areas

- Infrastructure (GitLab CI)
- Review workflow (MR pipelines)

### Risk Level

Medium - CI changes can block merges or introduce noisy failures if conditions are incorrect.

### Dependencies

- Access to current `.gitlab-ci.yml` structure and existing CI variables.
- No blocking task dependency declared in backlog.

Desired Outcome: `.gitlab-ci.yml` supports Codex code review in the appropriate CI context with explicit gating/conditions documented in this task.

<!--
SECTION:DESCRIPTION:END
-->

## Acceptance Criteria

<!-- AC:BEGIN -->
- [ ] #1 GitLab CI includes a Codex code review job (or equivalent integration job) defined in `.gitlab-ci.yml`.
- [ ] #2 The Codex job runs for merge request pipelines under explicit `rules`/conditions.
- [ ] #3 The Codex job does not run in unrelated pipeline contexts (for example, scheduled/tag pipelines) unless explicitly required.
- [ ] #4 Required environment variables/secrets are referenced via CI variables (no hardcoded tokens in repo).
- [ ] #5 Existing core CI jobs continue to run under their prior conditions (no unintended regressions in pipeline triggering).
- [ ] #6 The task notes include verification evidence showing the Codex job appears in the intended pipeline context.
<!-- AC:END -->

## Verification Plan

Automated:

- `nix develop -c nix run nixpkgs#yq -- --version` (tool availability sanity check if used for local CI YAML validation)
- `nix develop -c gitlab-ci-local --list` (if available) or equivalent local CI lint check

Manual:

- Open/update an MR and confirm Codex review job is present in MR pipeline.
- Confirm Codex job is absent from excluded pipeline sources per configured rules.
- Confirm no secrets were committed and CI variables are documented/referenced correctly.

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Inspect existing `.gitlab-ci.yml` stages, includes, and pipeline source rules.
2. Add Codex review job configuration with scoped MR `rules` and appropriate stage.
3. Wire required variables/inputs through CI variable references.
4. Validate CI syntax and job inclusion/exclusion behavior.
5. Record verification results in task notes.
<!-- SECTION:PLAN:END -->

## Follow-Up Tasks

- If broader CI refactor opportunities are discovered, create separate Backlog tasks.

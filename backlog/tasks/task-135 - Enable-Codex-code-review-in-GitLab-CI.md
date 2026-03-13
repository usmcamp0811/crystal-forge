---
id: TASK-135
title: Enable Codex code review in GitLab CI
status: Done
assignee: []
created_date: '2026-02-27 01:08'
updated_date: '2026-03-13 01:24'
labels: []
dependencies: []
priority: high
ordinal: 77000
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
1. Inspect existing `.gitlab-ci.yml` stages, includes, and pipeline source rules.
2. Add Codex review job configuration with scoped MR `rules` and appropriate stage.
3. Wire required variables/inputs through CI variable references.
4. Validate CI syntax and job inclusion/exclusion behavior.
5. Record verification results in task notes.
<!-- SECTION:PLAN:END -->

## Verification Plan

Automated:

- `nix develop -c nix run nixpkgs#yamllint -- .gitlab-ci.yml`

Manual:

- Open/update an MR and confirm Codex review job is present in MR pipeline.
- Confirm Codex job is absent from excluded pipeline sources per configured rules.
- Confirm no secrets were committed and CI variables are documented/referenced correctly.

## Follow-Up Tasks

- If broader CI refactor opportunities are discovered, create separate Backlog tasks.

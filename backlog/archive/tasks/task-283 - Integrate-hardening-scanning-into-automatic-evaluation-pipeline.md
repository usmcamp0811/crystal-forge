---
id: TASK-283
title: Integrate hardening scanning into automatic evaluation pipeline
status: Backlog
assignee: []
created_date: '2026-04-21 12:58'
labels:
  - enhancement
  - evaluation
  - hardening
  - architecture
dependencies:
  - TASK-276
documentation:
  - packages/default/src/hardening/scanner.rs
  - packages/default/src/services/evaluation_queue.rs
  - packages/default/src/services/hardening_scans.rs
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

Currently, hardening scans are manual operations triggered by users through the UI. This creates friction and means hardening data is not available by default when systems are evaluated.

## Goal

Make hardening scanning automatic and integrated into the evaluation pipeline, so that when a flake configuration is evaluated, its hardening data is also collected without requiring manual user action.

Specifically: tie hardening scanning into the dry-run evaluation that already happens, so that as a system becomes "eval'd" and ready to build, it already has its hardening scan completed.

## Proposed Approach

1. Merge hardening scan logic with the existing dry-run evaluation flow
2. Potentially combine relevant nix-eval-jobs invocations to reduce evaluation overhead
3. Update evaluation queue processing to automatically trigger hardening scans
4. Store hardening scan results as part of the evaluation result set
5. Update UI to show hardening data as part of normal eval results (remove manual "Run Hardening Scan" button)
6. Update database schema/workflow so hardening_scans table entries are created automatically during eval

## Current Implementation Context

- Hardening scanner: `packages/default/src/hardening/scanner.rs`
  - Uses `nix eval --apply` with tryEval wrapper to extract systemd service hardening options
  - Currently invoked manually via API endpoint
- Evaluation queue: `packages/default/src/services/evaluation_queue.rs`
  - Handles dry-run evaluations for flake configurations
  - Uses nix-eval-jobs for parallel evaluation
- Dry-run flow: checks if configurations can be built without actually building them

## Non-Goals

- Changing the hardening scoring algorithm or hardening option definitions
- Modifying the hardening UI presentation (already polished in TASK-276)
- Backfilling hardening data for historical evaluations (can be follow-up task)
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 When a flake configuration is evaluated, a hardening scan is automatically initiated without user action
- [ ] #2 Hardening scan results are available as soon as evaluation completes
- [ ] #3 UI shows hardening data by default for evaluated systems (no manual scan button needed)
- [ ] #4 Evaluation pipeline performance does not significantly degrade (ideally combines nix eval operations)
- [ ] #5 Database schema properly links hardening scans to evaluations
- [ ] #6 Existing manual scan functionality is removed or deprecated
- [ ] #7 Integration tests verify hardening data appears automatically after eval
<!-- AC:END -->

---
id: TASK-318
title: Add compliance export endpoints for OHDF/HDF and OSCAL assessment results
status: Backlog
assignee: []
created_date: '2026-05-24 02:34'
labels:
  - compliance
  - interop
  - export
  - backend
milestone: Compliance Foundations
dependencies:
  - TASK-317
references:
  - 'https://saf-cli.mitre.org/'
  - 'https://saf.mitre.org/libs/ohdf-converters/'
priority: medium
ordinal: 3170
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem
Compliance outcomes in Crystal Forge are not yet portable to external audit/review ecosystems (Heimdall/SAF/RMF workflows).

## Goal
Provide export capabilities for compliance bundle/control evaluations into OHDF/HDF-style and OSCAL assessment-results formats.

## Non-Goals
- No full bidirectional sync with external platforms.
- No import pipeline in this task.

## Architectural Constraints
- Export generation must read persisted evaluation/evidence records (no recomputation side effects).
- Include provenance and waiver metadata where format allows.
- Keep format adapters isolated from core evaluator logic.

## Verification Plan
- nix develop -c cargo check --package default
- nix develop -c cargo test --package default compliance::export
- nix develop -c cargo test --package default handlers::api::compliance_export

## Impact Areas
- packages/default/src/services/**
- packages/default/src/handlers/api/**
- packages/default/src/domain/**

## Risk Level
Medium
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 API endpoint(s) can export bundle/control evaluation results for selected systems/time windows.
- [ ] #2 OHDF/HDF-compatible export includes control status, key evidence metadata, and identifiers.
- [ ] #3 OSCAL assessment-results export includes control outcomes and relevant provenance/waiver fields.
- [ ] #4 Export validation tests ensure required fields are present and consistent with stored evaluations.
- [ ] #5 Documentation describes export scope and known mapping limitations.
<!-- AC:END -->

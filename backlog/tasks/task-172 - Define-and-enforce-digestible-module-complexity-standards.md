---
id: TASK-172
title: Define and enforce digestible module complexity standards
status: Backlog
assignee: []
created_date: '2026-03-04 22:16'
labels:
  - architecture
  - complexity
  - maintainability
dependencies: []
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem
We currently have many large files (for example ~29 files over 500 LOC), but CI complexity checks can still pass because they primarily gate on lint complexity violations rather than module digestibility. This creates risk of monolithic files while also risking over-fragmentation into hard-to-follow spaghetti code.

## Desired Outcome
Create a practical complexity policy and enforcement approach that keeps modules digestible without forcing unnecessary file splitting.

## Scope
- Review current complexity reporting and what is actually enforced in CI.
- Propose balanced thresholds/guidelines for:
  - file size (LOC)
  - function length
  - module cohesion (avoid arbitrary splitting)
- Define exceptions process for justified large modules.
- Propose CI/reporting updates so violations are visible and actionable.
- Recommend migration strategy for existing oversized files.

## Deliverables
- Written policy/guidelines document for code digestibility.
- Proposed CI gating/report behavior changes.
- Prioritized follow-up backlog tasks for refactors where needed.

## Notes
The goal is maintainable architecture, not metric gaming: avoid both monoliths and fragmented spaghetti modules.
<!-- SECTION:DESCRIPTION:END -->

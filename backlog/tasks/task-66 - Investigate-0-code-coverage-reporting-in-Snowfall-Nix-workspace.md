---
id: TASK-66
title: Investigate 0% code coverage reporting in Snowfall/Nix workspace
status: Backlog
assignee: []
created_date: '2026-02-20 01:56'
labels:
  - coverage
  - ci
  - testing
  - rust
  - nix
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Problem: The code coverage job is reporting 0% coverage, which suggests the coverage tooling may be scanning the wrong source location. This repository is a Snowfall flake and Rust sources live under packages/default rather than a conventional single-crate root layout.

Desired outcome: Determine whether coverage collection is targeting the correct crate/workspace paths, fix configuration so coverage reflects actual tested Rust code, and document the correct invocation/path assumptions for this repository structure.
<!-- SECTION:DESCRIPTION:END -->

## Problem Statement

Coverage reporting in CI currently shows 0%, which is not credible for the current test suite and indicates a likely mismatch between coverage collection configuration and repository layout.

## Goal

Make coverage collection target the actual Rust crates under `packages/default`, produce non-zero and reproducible reports in CI, and document the expected command pathing for this Snowfall flake structure.

## Non-Goals

- Replace the current coverage tool unless required to satisfy acceptance criteria.
- Expand this task into broad CI pipeline redesign.
- Add new product features unrelated to coverage reporting.

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Root cause of 0% coverage is identified and documented (for example: wrong working directory, wrong package selection, or incorrect source path filtering).
- [ ] #2 Coverage job/configuration is updated to run against the Rust workspace/crates under `packages/default`.
- [ ] #3 Running the documented coverage command in the repo environment generates a report with file entries from `packages/default`.
- [ ] #4 Coverage summary is non-zero when existing tests execute successfully (unless a reproducible edge case is explicitly documented).
- [ ] #5 Documentation is added/updated with the correct invocation and assumptions for Snowfall/Nix layout coverage execution.
<!-- AC:END -->

## Architectural Constraints

- Keep changes scoped to coverage tooling/configuration and related documentation.
- Do not introduce business logic changes in application modules.
- Follow deterministic repo workflows: use Nix-based commands and tracked files only.

## Verification Plan

Automated:

- `nix build .#checks.x86_64-linux.coverage`
- `nix develop -c cargo test --manifest-path packages/default/Cargo.toml`

Manual:

- Inspect generated coverage artifacts and confirm included source files are under `packages/default`.
- Validate coverage summary percentage is not 0% after test execution.

## Impact Areas

- Infrastructure (CI and Nix checks)
- Rust testing and coverage reporting
- Developer documentation for local/CI coverage workflows

## Risk Level

Medium - changes affect CI quality signals and could cause false positives/false negatives if misconfigured.

## Dependencies

- TASK-58 (historical baseline for current coverage job setup)

## Follow-Up Tasks

- If deeper coverage gaps are discovered by module, create separate Backlog tasks for targeted test improvements.

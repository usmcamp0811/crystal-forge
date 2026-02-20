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

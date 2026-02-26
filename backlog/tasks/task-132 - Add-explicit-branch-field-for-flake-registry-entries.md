---
id: TASK-132
title: Add explicit branch field for flake registry entries
status: Backlog
assignee: []
created_date: '2026-02-26 21:57'
labels: []
dependencies: []
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Problem: Flake sync currently infers branch from remote default, which is not deterministic for repos that track non-default branches. Desired outcome: Add an optional branch field to flake create/edit UI and backend model, auto-detect default branch for convenience, allow override, and make sync use persisted branch for deterministic behavior.
<!-- SECTION:DESCRIPTION:END -->

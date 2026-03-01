---
id: TASK-131
title: Flake credentials support for private sources
status: Backlog
assignee: []
created_date: '2026-02-26 21:49'
labels: []
dependencies: []
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Problem: Flake sync currently assumes public repository access and fails when credentials are required. Desired outcome: Add a secure UI and backend flow to provide and store per-flake credentials (for example token/SSH key references) so private flake URLs can be validated and synced without exposing secrets.
<!-- SECTION:DESCRIPTION:END -->

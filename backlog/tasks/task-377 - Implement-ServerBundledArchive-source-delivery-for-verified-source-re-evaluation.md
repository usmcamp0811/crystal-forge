---
id: TASK-377
title: >-
  Implement ServerBundledArchive source delivery for verified source
  re-evaluation
status: Backlog
assignee: []
created_date: '2026-07-03 15:49'
labels:
  - builder
  - remote-builds
  - architecture
  - source-delivery
dependencies: []
priority: high
ordinal: 322000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

The `source_re_evaluate_verified` strategy currently assigns `SourceInputDeliveryMode::LocalGitWorktree`, which requires the builder to run `git clone --bare <repo_url>` and `git fetch` directly from the remote Git repository. Builders may not be able to reach that location (private repos, network isolation, GovCloud), and the server is meant to be the source of truth for job artifacts.

## Goal

Implement `SourceInputDeliveryMode::ServerBundledArchive` so the server provides a self-contained source archive (via `nix flake archive` or `git archive`) that the builder downloads through the authenticated API, without needing direct Git remote access.

## Non-Goals

- Do not remove the existing `LocalGitWorktree` mode — it is valid for colocated deployments where builders and server share a mirror filesystem.
- Do not change the derivation evaluation/verification logic — only the source delivery mechanism.
- Do not change the `ServerDerivation` strategy — it already works correctly via derivation archive downloads.
<!-- SECTION:DESCRIPTION:END -->

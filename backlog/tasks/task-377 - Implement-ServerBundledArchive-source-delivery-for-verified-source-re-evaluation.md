---
id: TASK-377
title: >-
  Implement ServerBundledArchive source delivery for verified source
  re-evaluation
status: To Do
assignee: []
created_date: '2026-07-03 15:49'
updated_date: '2026-07-03 15:49'
labels:
  - builder
  - remote-builds
  - architecture
  - source-delivery
dependencies:
  - TASK-375.4
priority: high
ordinal: 3000
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

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Server endpoint GET /api/v1/builders/:id/jobs/:job_id/source-archive streams a tar archive of the source at the exact commit the server evaluated
- [ ] #2 Server generates the archive from its existing bare mirror (git archive <commit_hash>) on-demand when a source_re_evaluate_verified job is claimed with ServerBundledArchive mode
- [ ] #3 Builder downloads the archive through the authenticated API instead of running git clone --bare, extracts it to a local directory, and evaluates from that directory
- [ ] #4 ensure_mirror_has_commit / ensure_source_worktree are bypassed when delivery mode is ServerBundledArchive - builder uses the downloaded archive instead
- [ ] #5 Source archive endpoint respects builder authentication and session ownership (same signing as all builder API calls)
- [ ] #6 Archive is cleaned up server-side after job completion/failure
- [ ] #7 Operator can configure source delivery mode per-system (LocalGitWorktree or ServerBundledArchive) in server config
- [ ] #8 Unit tests for the archive endpoint and builder extraction logic
- [ ] #9 Existing source_re_evaluate_verified tests continue to pass with LocalGitWorktree unchanged
<!-- AC:END -->

## Definition of Done
<!-- DOD:BEGIN -->
- [ ] #1 New server endpoint serves source archive at the authorized commit
- [ ] #2 Builder downloads and extracts archive instead of git clone when mode is ServerBundledArchive
- [ ] #3 ensure_source_worktree and ensure_mirror_has_commit are bypassed in archive mode
- [ ] #4 Archive endpoint is authenticated and session-checked like all builder endpoints
- [ ] #5 Config switch for source delivery mode works (LocalGitWorktree vs ServerBundledArchive)
- [ ] #6 sqlx metadata is updated if new queries are added
- [ ] #7 rustfmt passes on changed files
- [ ] #8 nix flake check is left to CI
<!-- DOD:END -->

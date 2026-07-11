---
id: TASK-390
title: 'Remote builders: complete end-to-end fix (source delivery + drv transport + materialization)'
status: Review
assignee: []
created_date: '2026-07-10 00:00'
updated_date: '2026-07-10 00:00'
labels:
  - builders
  - remote-builders
  - source-delivery
  - architecture
  - backend
dependencies:
  - TASK-375.4
  - TASK-384
priority: high
ordinal: 390000
---

## Description

Umbrella task covering all remote builder fixes in a single MR for cohesive
deployment and verification. Absorbs TASK-377 WIP, TASK-375.3, and TASK-387.

### Three problems fixed together

**1. Source delivery (TASK-377 / TASK-387):** Remote builders currently run
`git clone --bare` / `git fetch` directly from Git remotes. Server must instead
package a source archive (`tar` of the bare mirror) and serve it via an
authenticated API endpoint. Builders download, verify SHA-256, extract, and
evaluate from the local archive — no Git remote access needed.

**2. Drv closure transport (TASK-375.3):** The current `ServerDerivation` path
downloads the entire `.drv` closure as one monolithic binary blob
(`nix-store --export`) buffered in server RAM. Replace with
substituter-pull-first: builder calls `publish_derivation_closure` so server
pushes to Attic/cache asynchronously, then builder pulls via normal Nix
substituters. Monolithic archive remains as explicit fallback only.

**3. Materialization failure states (TASK-375.3):** Jobs stuck in `building`
when drv or source materialization fails. Add explicit `path_materialization_failed`
attempt failure phase so jobs terminate cleanly instead of staying stuck.

## Acceptance Criteria

- [ ] Server endpoint `GET /api/v1/builders/:id/jobs/:job_id/source-archive` streams (not buffers) source tar.gz
- [ ] Server generates source archive from bare mirror at job-claim time for `ServerBundledArchive` mode
- [ ] `verified_source_identity_for_derivation()` populates `archive_url` and `archive_sha256` when mode is `ServerBundledArchive`
- [ ] Builder downloads archive, verifies SHA-256, extracts to mirror, evaluates without git clone
- [ ] `ensure_derivation_available`: tries substituter publish+pull first; falls back to archive only on failure
- [ ] Monolithic drv archive download is explicit last-resort, not default path
- [ ] Materialization failures report `path_materialization_failed` and don't leave job `building`
- [ ] Server mirror clone uses flake credentials from DB for private repos
- [ ] `cargo check` passes clean
- [ ] Targeted unit tests pass
- [ ] No regressions in existing `LocalGitWorktree` or `ServerDerivation` paths

## Implementation Notes

LOCK: opencode on dev in ~/code/crystal-forge/TASK-390-remote-builders-complete
MR: https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/297

## Notes

Absorbs TASK-377 WIP (gpt-5.5 in TASK-377-server-bundled-archive worktree).
TASK-375.3 (To Do), TASK-377 (In Progress), TASK-387 (Backlog) all closed by this MR.

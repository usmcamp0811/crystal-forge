---
id: TASK-484
title: Make cold-input NixOS configuration discovery reliable
status: In Progress
assignee:
  - '@openai-agent'
created_date: '2026-10-12 14:29'
updated_date: '2026-10-12 14:30'
labels: []
dependencies: []
references:
  - packages/default/crates/cf-server/src/flake/commits.rs
  - packages/default/crates/cf-server/src/models/evaluate_with_policies.rs
  - packages/default/crates/cf-server/src/flake/verified_source.rs
priority: high
type: bug
ordinal: 506000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
User requests a separate evaluation reliability fix based on TASK-470's committed worktree head; TASK-470 is expected to merge before this fix. CF ata-nix-config preflight at exact commit 3d6b517bd2b999a21814df3df4eb498202c4e2bd failed configuration discovery after 60s with cleanup confirmed=false before nix-eval-jobs. Manual local cold discovery succeeded in 145.41s with 12 systems and warm retry took 1.169s; CF succeeded after cache warming. A second Campground local cold discovery took 170.34s, fetched locked dotfiles c56cc6262ad191749226c34dc0c91f00959c340a (322.43 MiB) and CF a33c1d0cb534ee0a78c41e015a7c75e4c845df28 (78.64 MiB), and reported an unauthorized CUDA Cachix cache. Local user/checkout commands are not proof of identical CF runtime behavior. Investigate precise root cause and make valid cold-input flakes proceed without manual cache warming. Latest user explicitly redirects work to timeout now; leave pending Attic metrics work intact and do not finish its checks first. Do not merge or change cache/signing/deployment/derivation identity; preserve adaptive memory-aware recovery and fail-closed cleanup/source integrity.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Authoritative configuration discovery uses the same exact verified immutable commit content and NAR-qualified source as policy evaluation without redundant root Git resolution or revision fallback; lock integrity and private input credentials remain enforced
- [ ] #2 Discovery has a configurable bounded outer process deadline with a cold-input-appropriate default and explicit upper limit independently of Nix realization and bulk evaluator timeouts
- [ ] #3 Discovery resource admission preserves PostgreSQL advisory-lock-before-semaphore order without nested deadlocks; unconfirmed child cleanup retains ownership and prevents replacement overlap
- [ ] #4 Timeout and cancellation provide bounded cleanup acknowledgment while preserving process-group isolation direct-child reap and fail-closed quarantine
- [ ] #5 Preflight logs distinguish evidenced source/input-resolution and discovery stages completion/system count timeout cleanup status and actual Nix errors without credential leakage; terminal failure does not remain indefinitely In Progress
- [ ] #6 Deterministic local cold/warm and private-input fixtures plus Rust regressions cover deadline source identity lock integrity concurrency cancellation cleanup quarantine discovery-to-policy success genuine errors and terminal status
- [ ] #7 Relevant evaluator/server integration and VM checks are run with exact results; documentation root-cause findings safety review commit pushed MR and exact-head CI evidence are recorded without merging
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Inspect authoritative preflight and artifact discovery call chains, verified source identity, credential environment, configuration and process ownership. Establish facts separately from cold-fetch hypotheses.
2. Reuse the verified NAR-qualified source for authoritative name discovery. Add a bounded discovery-specific server timeout (default 300s with explicit maximum) and match evaluator source/lock/purity/IFD policy.
3. Admit cold discovery under existing advisory-lock-before-semaphore ownership scope without nested locks. Retain ownership through bounded cleanup/quarantine. Give timed-out bounded child cleanup a separate small acknowledgment opportunity without releasing unconfirmed ownership.
4. Add evidence-based bounded/redacted preflight diagnostics and verify terminal failure status. Start isolated task live preview before browser-visible workflow edits; report exact startup blocker if unavailable.
5. Add deterministic source/lock/auth cold-warm/error/deadline/concurrency/cancel/quarantine regressions, run affected Rust and VM checks, review deadlocks/leaks/source parity and update source/knowledge documentation.
6. Commit/push separate fix normally; open MR with TASK-470 merge dependency and exact-head CI outcomes. Do not merge.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
LOCK: openai-agent on sledge in /home/mcamp/code/crystal-forge/TASK-484-discovery-timeout

Preflight: clean dedicated branch TASK-484-discovery-timeout at TASK-470 committed head a33c1d0cb534ee0a78c41e015a7c75e4c845df28. No main worktree registered. dev has preserved untracked TASK-478 lock and session-ses_f927.md; no integration writes or cleanup. Pending TASK-470 metrics files remain intact and excluded. User latest instruction prioritizes timeout now over earlier metrics-finalization sequence. Risk high: source identity and subprocess ownership. Merge dependency: TASK-470 must land in dev before this branch merges; implementation can proceed on its committed head. Verification: targeted config/discovery/process Rust tests; deterministic local cold/warm/private/immutable-source fixture and relevant verified-source/evaluator/server VM checks. No external GitLab download-speed test.
<!-- SECTION:NOTES:END -->

---
id: TASK-375.4
title: 'Remote builders: add verified source re-evaluation strategy'
status: In Progress
assignee:
  - '@gpt-5.5'
created_date: '2026-06-30 17:46'
updated_date: '2026-07-01 02:11'
labels:
  - builder
  - remote-builds
  - architecture
  - nix
  - hotfix-followup
milestone: Builder API hotfix
dependencies:
  - TASK-375
references:
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/289'
modified_files:
  - packages/default/src/models/builders.rs
  - packages/default/src/builder/api_client.rs
  - packages/default/src/bin/builder.rs
  - packages/default/src/handlers/api/builders.rs
  - packages/default/src/queries/builders.rs
  - modules/nixos/crystal-forge/default.nix
  - packages/default/src/config/builder.rs
  - packages/default/src/config/server.rs
  - packages/default/src/derivations/mod.rs
  - docs/multi-builder-api.md
parent_task_id: TASK-375
priority: high
ordinal: 5520
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Problem: the current `server_derivation` strategy preserves strong server authority but requires robust store-path transport to remote builders. A source-based builder strategy can reduce store-closure transport pressure, but an unverified source checkout would weaken Crystal Forge's guarantee that policy was applied to the exact derivation being built.

Goal: add an explicit verified source re-evaluation strategy (`source_re_evaluate_verified` / `source_verified`). In this mode, the server still evaluates first and records the authoritative toplevel derivation path (`.drvPath`) as the policy/authorization identity. The builder then obtains immutable server-provided source, evaluates locally under controlled Nix settings, compares its locally evaluated toplevel `.drvPath` to the server-expected `.drvPath`, and builds only if the strings match.

Key design point: compare the toplevel derivation path, not an evaluation blob or full closure. For an input-addressed derivation, matching `/nix/store/<hash>-...drv` transitively verifies the build plan graph the builder is about to build is the same plan the server authorized. This verifies derivation identity/build plan equality, not bit-for-bit output reproducibility.

Required execution order: eval before build. The builder must first run the equivalent of `nix eval --raw .#nixosConfigurations.<host>.config.system.build.toplevel.drvPath`, compare that string to the server-provided expected `.drvPath`, and only then build the exact verified derivation, e.g. `nix build "$drv^*"`. Do not run a normal `nix build` first and inspect after the fact, because that cooks before checking and reintroduces a verification/execution gap.

Server-side derivation identity should come from pure evaluation (`nix eval --raw ...drvPath`), not `nix build --dry-run`. A dry-run build resolves more information than needed; the strategy only needs the authoritative `.drvPath` string for comparison.

Recommended source delivery: avoid broad/reusable Git credentials on builders. Prefer a server-fetched immutable source archive or NAR/flake archive with hash, commit, flake target, lock metadata, and evaluator fingerprint. Builders should not need direct Postgres access, ambient source credentials, or mutable branch names. Decide explicitly whether builders may fetch public flake inputs themselves or whether the server bundles inputs with `nix flake archive`; the tighter locked-down/GovCloud-friendly option is server-bundled inputs so builders need zero internet and zero Git credentials for evaluation.

Non-Goals:
- Do not make unverified source checkout a production strategy.
- Do not silently fall back from `server_derivation` to verified source re-evaluation or vice versa.
- Do not distribute broad/reusable Git credentials to builders.
- Do not claim output reproducibility; this task verifies derivation identity/build plan equality before build, not bit-for-bit output reproducibility.
- Do not remove `server_derivation` support in this task.
- Do not solve all cache/substituter transport concerns for output/input dependency substitution; builders still use approved substituters for normal Nix dependency fetching.

Architectural Constraints:
- Strategy selection must be explicit per job/builder capability/scheduler policy.
- Server remains authoritative for queueing, policy, and expected derivation identity.
- Builder must compare actual local evaluation result against server-provided expected toplevel `.drvPath` before building.
- Builder must build the verified derivation object after the match, avoiding a gap between verification and execution.
- Source identity must be immutable: prefer source archive/NAR URL plus hash over branch names; include git commit, flake target, lock hash/metadata, and evaluator fingerprint for auditability.
- Builder evaluation must use controlled Nix settings: no lockfile mutation, pure evaluation where feasible, explicit experimental features, recorded Nix version/evaluator fingerprint, and no ambient credentials.
- Private source access should use short-lived/job-scoped source archives or tokens rather than long-lived Git credentials on every builder.
- Source/input delivery mode must be explicit: either server-bundled flake inputs for locked-down builders, or builder-fetched public inputs where that is acceptable.
- Mismatches must fail before build with a distinct `derivation_mismatch` error class/phase.
- Fleet configuration should pin/record Nix versions across server and builders so normal evaluation does not drift silently.

Impact Areas:
- Builder capability advertisement
- Build job manifest/schema
- Scheduler strategy selection
- Source archive/access API
- Builder local evaluation runner
- Attempt phase/error model
- Documentation and operator guidance

Risk Level: high

Verification Plan:
- Unit tests for job manifest serialization/deserialization for `source_re_evaluate_verified` / `source_verified`.
- Unit tests for derivation identity comparison and mismatch error classification.
- Integration test where builder evaluates immutable source, matches expected `.drvPath`, builds the verified derivation, and reports success or build failure.
- Integration test where expected `.drvPath` differs from locally evaluated `.drvPath`; job fails before build with `derivation_mismatch` before any build starts.
- Test that builder performs eval/compare before build and builds the verified `.drv` path rather than invoking an unverified attr build first.
- Source delivery/security test or config review confirming builders do not receive broad Git credentials and evaluation runs without ambient secret environment variables.
- Test or assertion that lockfile mutation/impure evaluation is disabled or explicitly rejected for this strategy.
- Targeted `nix develop` cargo checks/tests for changed crates; run heavier Nix checks only if Nix modules/packaging are modified.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A new explicit verified source strategy exists (`source_re_evaluate_verified` or agreed final name) and is not used as a silent fallback.
- [ ] #2 Server derives the authoritative fingerprint using pure evaluation of the target `.drvPath` (`nix eval --raw ...drvPath` equivalent), not a dry-run build.
- [ ] #3 Server job manifest includes immutable source identity, flake target, lock/source metadata, evaluator fingerprint fields, source/input delivery mode, and expected toplevel `.drvPath`.
- [ ] #4 Builder obtains immutable server-provided source without broad/reusable Git credentials and evaluates it with controlled Nix settings.
- [ ] #5 Builder compares locally evaluated toplevel `.drvPath` to the server-expected `.drvPath` before any build starts and refuses to build on mismatch.
- [ ] #6 After a successful match, builder builds the exact verified derivation object and reports logs, progress, completion/failure, and output path through the API-only builder protocol.
- [ ] #7 Derivation mismatch, source fetch failure, evaluation failure, and input/source availability failures are represented as distinct attempt phases/error classes and do not leave jobs stuck in `building`.
- [ ] #8 Unverified source checkout support is absent or clearly limited to development/testing only; there is no hidden fallback between strategies.
- [ ] #9 Operator documentation explains when to use verified source re-evaluation, source/input delivery options (`nix flake archive`/bundled inputs vs public input fetching), security requirements, Nix version/purity expectations, and the distinction between derivation identity verification and output reproducibility.
- [ ] #10 Builder source acquisition uses a local mirror/snapshot model: server serves enough metadata/artifact information for the builder to keep a local flake copy at the authorized commit, and colocated server/builder deployments can share the same configured mirror root without duplicate checkouts.
- [ ] #11 Verified source mode uses detached Git worktrees at exact commit SHAs from a local mirror/worktree root, verifies the worktree HEAD matches the manifest commit before eval, and cleans up job/commit worktrees after build completion and cache-push/reporting lifecycle completes.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Refinement: implement source workspace metadata around a bare mirror plus detached worktree at the authorized commit. Add manifest fields/config for mirror/worktree roots and cleanup behavior. Builder source acquisition should resolve/create a detached commit worktree, verify HEAD equals the job commit, eval from that path, and schedule/remove the worktree after build and cache push reporting complete.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Further design refinement: use Git mirror/worktree semantics for verified source acquisition. Maintain a shared bare mirror per flake/repo and create detached worktrees at exact authorized commit SHAs for evaluation/build. Colocated server and builder can point at the same mirror/worktree roots; remote builders keep equivalent local mirrors from server-provided source metadata/artifacts. Builders must verify worktree HEAD equals the manifest commit before eval. Worktrees should be cleaned up after build completion and cache-push/reporting lifecycle is done.
<!-- SECTION:NOTES:END -->

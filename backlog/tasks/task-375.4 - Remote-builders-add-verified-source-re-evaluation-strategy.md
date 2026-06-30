---
id: TASK-375.4
title: 'Remote builders: add verified source re-evaluation strategy'
status: To Do
assignee: []
created_date: '2026-06-30 17:46'
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
parent_task_id: TASK-375
priority: high
ordinal: 5520
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Problem: the current `server_derivation` strategy preserves strong server authority but requires robust store-path transport to remote builders. A source-based builder strategy could reduce store-closure transport pressure, but an unverified source checkout would weaken Crystal Forge's guarantee that policy was applied to the exact derivation being built.

Goal: add an explicit `source_re_evaluate_verified` remote-builder strategy. In this mode, the server still evaluates and records the expected derivation identity first, then the builder obtains immutable source, evaluates locally under a controlled evaluator configuration, and builds only if the locally produced derivation identity matches the server-authorized expected derivation.

Non-Goals:
- Do not make unverified source checkout a production strategy.
- Do not silently fall back from `server_derivation` to source re-evaluation.
- Do not distribute broad/reusable Git credentials to builders.
- Do not claim output reproducibility; this task verifies derivation identity before build, not bit-for-bit output reproducibility.
- Do not remove `server_derivation` as the default production mode.

Architectural Constraints:
- Strategy selection must be explicit per job/builder capability/scheduler policy.
- Builder must compare actual local evaluation result against server-provided expected derivation identity before building.
- Source identity must be immutable: prefer source archive/NAR URL plus hash over branch names; include git commit and lock hash for auditability.
- Builder evaluation must use controlled Nix settings: no lockfile mutation, pure evaluation where feasible, explicit experimental features, recorded Nix version/evaluator fingerprint, and no ambient credentials.
- Private source access should use short-lived/job-scoped source archives or tokens rather than long-lived Git credentials on every builder.
- Mismatches must fail before build with a distinct `derivation_mismatch` error class/phase.

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
- Unit tests for job manifest serialization/deserialization for `source_re_evaluate_verified`.
- Unit tests for derivation identity comparison and mismatch error classification.
- Integration test where builder evaluates immutable source and matches expected `.drv`, then builds/reports success or build failure.
- Integration test where expected `.drv` differs from locally evaluated `.drv`; job fails before build with `derivation_mismatch`.
- Security-oriented test/config review confirming builders do not receive broad Git credentials and evaluation runs without ambient secret environment variables.
- Targeted `nix develop` cargo checks/tests for changed crates; run heavier Nix checks only if Nix modules/packaging are modified.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A new explicit `source_re_evaluate_verified` strategy exists and is not used as a silent fallback.
- [ ] #2 Server job manifest includes immutable source identity, flake target, evaluator fingerprint fields, and expected derivation identity.
- [ ] #3 Builder locally evaluates the source and refuses to build when the actual derivation identity does not match the server-expected derivation identity.
- [ ] #4 Successful verified source re-evaluation builds still report logs, progress, completion/failure, and output path through the API-only builder protocol.
- [ ] #5 Derivation mismatch, source fetch failure, and evaluation failure are represented as distinct attempt phases/error classes and do not leave jobs stuck in `building`.
- [ ] #6 Production default remains `server_derivation`; any `source_checkout_unverified` support is absent or clearly limited to development/testing only.
- [ ] #7 Operator documentation explains when to use `source_re_evaluate_verified`, its security requirements, and its limits.
<!-- AC:END -->

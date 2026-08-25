---
id: TASK-433.5
title: 'TASK-433 Phase 4: Composite heterogeneous enforcement execution'
status: In Progress
assignee:
  - '@opencode-agent'
created_date: '2026-08-23 01:42'
updated_date: '2026-08-25 01:57'
labels:
  - design-parity
  - policy
  - enforcement
  - server
  - phase-4
dependencies:
  - TASK-433.4
references:
  - TASK-433
  - TASK-433.1
documentation:
  - docs/design/CrystalForge/data-enforcement.js
parent_task_id: TASK-433
priority: high
type: feature
ordinal: 437000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Objective
Parent umbrella: TASK-433, phase 4 of 8 (contextual only). Implements complete backward-compatible enforcement execution across all visible enforcement kinds at the correct evaluation phase, with full DTO -> validation -> storage -> execution -> result/evidence -> read-back -> import/export coverage.

## Explicit scope
- Verify and correctly phase-execute: `nixos_option`, `packages_installed`, `packages_absent`, `custom_eval`, `cve_block`, `eval_passed`, `pin_required`, `time_window`, `approval_required`, `rollout_percent` at the correct evaluation/package, scan/build, source/evaluation or deployment phase.
- Every visible enforcement kind has a complete UI -> DTO -> validation -> storage -> execution -> result/evidence -> read-back/import/export path, or is hidden if incomplete.
- Mixed Nix/evaluation-phase plus non-Nix rule sets (e.g. Nix option rule plus CVE-block rule in one policy) persist and evaluate together with all-semantics aggregation and visible per-rule outcomes.
- Recommendations remain suggestions only (never silently enforced).
- For every exposed enforcement kind: tests cover create, validate, persist, reload, correct phase, pass, fail, error/not-checked, edit, evidence and import/export.

## Explicit non-scope
No POA&M changes. Do not flatten phases into Nix. Do not weaken Pass/Fail semantics.

## Verification
```bash
nix develop -c cargo fmt --manifest-path packages/default/Cargo.toml -- --check
nix develop -c cargo test --manifest-path packages/default/Cargo.toml
nix build .#packages.x86_64-linux.server --no-link
nix build .#checks.x86_64-linux.server-regressions --no-link
```
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Every visible enforcement control has a complete DTO, validation, storage, execution, result/evidence, read-back and import/export path or is hidden.
- [ ] #2 Mixed Nix/evaluation-phase plus non-Nix rule sets persist and evaluate with all semantics and visible constituent outcomes.
- [ ] #3 For every exposed enforcement kind tests cover create, validate, persist, reload, correct phase, pass, fail, error/not-checked, edit, evidence and import/export.
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Phase-4 preflight began from clean shared task worktree `/home/mcamp/code/crystal-forge/TASK-433-policy-poam-workflows`, branch `TASK-433-policy-poam-workflows`, HEAD/base branch history containing reviewed Phase-3 remediation `34ceb84a`. Exact Phase-3 MR pipeline 2787525864 was verified successful at that SHA. No Phase-4 implementation commit exists after `34ceb84a`; Phase 5 remains out of scope.
<!-- SECTION:NOTES:END -->

---
id: TASK-433.5
title: 'TASK-433 Phase 4: Composite heterogeneous enforcement execution'
status: In Progress
assignee:
  - '@opencode-agent'
created_date: '2026-08-23 01:42'
updated_date: '2026-08-25 13:08'
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

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
## Phase 4 implementation plan

1. Preserve legacy policy execution unchanged and fix the current enforced-composite loader fallback so malformed/unsupported enforced policies cannot degrade to an empty policy set.
2. Keep composite schema version 1 and stable rule UUIDs. Add typed variants only for kinds completed in this phase: `packages_absent`, `eval_passed`, `pin_required`, and `time_window`, alongside the existing four. Keep `approval_required` and `rollout_percent` hidden because repository inspection found no authoritative exact-target approval workflow and no production caller that advances canary phases; exposing either would create a control that saves but does not safely govern deployment. Do not add arbitrary JSON variants or start TASK-415/Phase 5.
3. Add a phase-neutral outcome model with explicit Pass/Fail/Error/NotChecked, stable policy-version/rule identity, phase, detail, evidence, and deterministic `all` aggregation. Add additive normalized assessment/rule-result persistence scoped by exact system, derivation/target, policy lineage/version, and effective config/set identity, with guarded transactional phase merges so later phases preserve earlier outcomes and stale versions/results cannot authorize deployment.
4. Extend the Nix/config executor for `nixos_option`, `packages_installed`, `packages_absent`, and `custom_eval` using stable rule-ID keys, shared legacy package-name semantics, target-authoritative dynamic option lookup, one safe typed semantic-value-to-Nix encoder, and contained try-eval/non-boolean Error outcomes. Record `eval_passed` from real per-configuration evaluator completion/failure and `pin_required` from Nix-resolved immutable source revision metadata rather than display strings.
5. Add scan-phase `cve_block` evaluation against the newest scan attempt for the exact derivation. Missing/active is NotChecked, failed/unavailable is Error, completed counts evaluate Pass/Fail; batch/load once per derivation and persist scan identity/evidence.
6. Add deployment-phase `time_window` through the existing timezone-aware service with strict create/import validation and deterministic injected-time tests. Create one authoritative final composite authorization aggregate consumed by deployment entry points; Fail/Error/due-phase NotChecked blocks. Preserve explicit legacy behavior and keep approval/rollout controls hidden.
7. Expose constituent outcomes through the smallest existing API/evidence surfaces and Web UI result presentation: overall status plus ordered rule kind/phase/status/detail/evidence. Update Add Enforcement recommendations/chooser so only the eight complete kinds are visible and recommendations remain suggestions.
8. Preserve JSON/TOML/CF-native interchange envelopes and add exact round-trip tests for every exposed kind. Add create/validate/persist/reload/edit/phase/pass/fail/error-or-notchecked/evidence/import-export coverage, plus the central mixed `nixos_option + cve_block + time_window` lifecycle regression and failure permutations.
9. Update enforcement architecture documentation, SQLx metadata if schema/checked queries change, authoritative browser workflow, and the task's per-kind coverage matrix.
10. Run all required server/Web UI/Nix/SQLx/browser/flake gates. Then stop coding for independent requirements, data integrity, performance, UI/error-state, E2E, and regression-adequacy review. Resolve every P0/P1/P2 before checking exactly AC1-AC3, moving TASK-433.5 to Review, updating MR !318, committing/pushing, and stopping before Phase 5.

Independent evaluator/security review remediation (2026-08-25): keep changes limited to evaluator/domain backend files. Replace invalid option lookup with strict quoted-path parsing plus fold-based target config traversal; make bulk/standalone policy expressions share canonical `config`; contain malformed composite custom_eval by authoritative Nix parse validation and a static Error result rather than embedding invalid syntax; compare exact requested and Nix-resolved immutable revisions and fix flake rev replacement; derive eval_passed outcomes from terminal evaluation/error helpers; add a dedicated Nix string encoder and direct environment.systemPackages pname contract/validation; classify malformed enforced policy data as deterministic while DB/infrastructure loader failures remain transient and restore legacy conflict behavior. Verify with cargo fmt, focused CARGO_BUILD_JOBS=1 Rust tests, and ignored authoritative Nix evaluation tests.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Phase-4 preflight began from clean shared task worktree `/home/mcamp/code/crystal-forge/TASK-433-policy-poam-workflows`, branch `TASK-433-policy-poam-workflows`, HEAD/base branch history containing reviewed Phase-3 remediation `34ceb84a`. Exact Phase-3 MR pipeline 2787525864 was verified successful at that SHA. No Phase-4 implementation commit exists after `34ceb84a`; Phase 5 remains out of scope.

## Pre-implementation enforcement coverage matrix (current head `34ceb84a`)

| Kind | UI | DTO/validation | Storage/read-back/import-export | Existing executor/result/evidence | Phase-4 decision |
|---|---|---|---|---|---|
| nixos_option | visible | typed composite | exact JSONB + JSON/TOML/CF-native envelope | composite rejected; no constituent outcome | implement target-authoritative Nix/config execution |
| packages_installed | visible | typed composite | exact envelope | only legacy require_packages expression; no composite outcome | implement shared package executor |
| packages_absent | absent | none | none | none | add typed complete path |
| custom_eval | visible | typed composite | exact envelope | legacy custom_check only; no contained constituent Error | implement contained Nix/config execution |
| cve_block | visible | typed composite | exact envelope | legacy CVE paths are incompatible/global or broken; no persisted outcome | implement exact-derivation scan executor |
| eval_passed | hidden UI-only | no composite variant | none | evaluator has per-system success/failure but no rule outcome | add typed complete path from evaluator state |
| pin_required | absent | none | none | requested commit exists but Nix-resolved source revision is not persisted | add typed path using resolved source metadata |
| time_window | hidden UI-only | legacy standalone config only | legacy envelope only | timezone-aware service exists; transient auto-latest gate only | add typed complete path and centralized final authorization |
| approval_required | hidden UI-only | legacy standalone config only | legacy envelope only | mutable commit+lineage approval rows lack exact target/version, immutable audit, environment authorization, and delivery authorization | remain hidden; document architecture gap, do not fabricate completion |
| rollout_percent | hidden UI-only | legacy standalone config only | legacy envelope only | canary state scaffolding exists but `complete_phase` has no production caller, so rollout cannot advance | remain hidden; document architecture gap, do not fabricate completion |

Cross-cutting findings to correct: current composite loader errors are caught and replaced with an empty policy set; persisted evaluation results use policy-version UUIDs while compliance lookup uses lineage UUIDs; deployment-phase outcomes are transient; derivation-only result JSON is not safe for system-specific multi-phase accumulation; manual/rollback target writes bypass advanced gates; no existing constituent outcome model distinguishes Fail/Error/NotChecked. No production code was modified before recording this matrix and plan.
<!-- SECTION:NOTES:END -->

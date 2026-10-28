---
id: TASK-460
title: >-
  Post-TASK-440 Follow-up 4: Reduce primary fast evaluator after policy
  consumers migrate to Config artifact evidence
status: To Do
assignee: []
created_date: '2026-09-06 23:09'
labels: []
dependencies:
  - TASK-440
  - TASK-458
  - TASK-457
documentation:
  - >-
    backlog/docs/doc-24 -
    Crystal-Forge-Evaluation-Evidence-Build-Admission-and-Deployment-Gating-Architecture.md
priority: high
type: enhancement
ordinal: 469000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

The primary fast `nix-eval-jobs` evaluator currently computes some configuration-policy facts that duplicate what the TASK-440 Config Inspector now computes as canonical evidence. Once Follow-up Task 2 (migrate config-derived policies to Config artifact evidence) and Follow-up Task 3 (progressive build admission modes) have shipped, this duplicate work in the bulk evaluator should be removed so the fast evaluator returns to its narrow, fleet-safe scope. This is "Follow-up Task 4 — Reduce primary evaluator" from the "Crystal Forge Evaluation, Evidence, Build Admission, and Deployment Gating Architecture" design document (doc-24, §3, §17 item 10, §21).

This task must run after Follow-up Task 2 (TASK-458) and Follow-up Task 3 (TASK-457) so that no build-admission mode or migrated policy still depends on a fast-evaluator fact that this task removes.

## Non-goals

- Do not remove `cfAgentEnabled` (or an equivalent narrowly-whitelisted agent-capability fact) from the fast evaluator; per design §3, this fact is an explicit fast-evaluator responsibility needed for early build admission (CF-enabled mode) and must remain.
- Do not remove exact flake/revision identity, configuration name, `config.system.build.toplevel`/derivation identity, or expected NixOS output/store identity from the fast evaluator; these are core Tier-1 responsibilities per design §3.
- Do not weaken drift classification; per design §6, drift classification must continue to depend only on Tier-1 identity facts and persisted agent-reported state, with no regression in latency.
- Do not perform this reduction if Follow-up Task 2's policy migration is incomplete for a policy type that still reads the fact being removed; verify no remaining consumer before removing a fact.

## Scope

1. Identify every configuration-policy fact currently computed by the primary fast evaluator that duplicates evidence now available from `ConfigArtifactV2` after Follow-up Task 2's migration (e.g. any `cfg.options` traversal, `_module.graph` exploration, arbitrary option-tree introspection, provenance extraction, or general custom policy expression evaluation that crept into the bulk evaluator).
2. Confirm, for each identified fact, that no remaining policy, build-admission mode (Follow-up Task 3), or other consumer still depends on the fast evaluator computing it; if a consumer remains, do not remove that fact in this task and record it as a blocked/out-of-scope item.
3. Remove the confirmed-duplicate facts from the primary bulk evaluator, keeping only: fast identity/build facts (flake/revision identity, configuration name, derivation/store identity) and the smallest necessary early-admission facts (`cfAgentEnabled` or equivalent).
4. Verify eval-to-drift and eval-to-build-admission latency does not regress relative to the pre-reduction baseline, per design §17 performance goals #1-#3.
5. Verify the fast evaluator's search space does not include `cfg.options`, `_module.graph`, `flake.nixosModules`, `lib.evalModules`, or similar broad exploration after the reduction, per design §17 performance goal #10.

## Verification plan

- Before/after latency measurement for time-to-expected-system-identity and time-to-drift-classification on a representative fleet-sized evaluation run, confirming no regression per design §17 goals #1-#2.
- `nix develop -c cargo check --package cf-server` (or the applicable evaluator package) and targeted tests proving the fast evaluator's output no longer includes the removed duplicate facts, while `cfAgentEnabled`, identity, and derivation facts are unchanged.
- Regression tests confirming build-admission modes (Follow-up Task 3) and migrated policies (Follow-up Task 2) continue to function correctly with the reduced fast evaluator output.
- Static/structural check (or equivalent Nix-expression review) confirming the fast evaluator expression does not traverse `cfg.options`, `_module.graph`, `flake.nixosModules`, or `lib.evalModules` beyond what is strictly required for Tier-1 identity facts.
- Confirm no drift-classification behavior change and no regression in existing evaluator-snapshot-isolation or config-inspector Nix checks.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Every configuration-policy fact currently duplicated between the primary fast evaluator and ConfigArtifactV2 evidence is identified, with each fact confirmed to have no remaining consumer before removal.
- [ ] #2 Duplicate configuration-policy evaluation is removed from the primary bulk evaluator; only fast identity/build facts (flake/revision identity, configuration name, derivation/store identity) and the smallest necessary early-admission fact (cfAgentEnabled or equivalent) remain.
- [ ] #3 Time to expected system identity and time to drift classification show no regression relative to the pre-reduction baseline, measured on a representative fleet-sized evaluation run.
- [ ] #4 Drift classification continues to depend only on Tier-1 identity facts and persisted agent-reported state, with unchanged behavior.
- [ ] #5 The fast evaluator's Nix expression does not traverse cfg.options, _module.graph, flake.nixosModules, lib.evalModules, or equivalent broad exploration beyond what Tier-1 identity facts strictly require.
- [ ] #6 Build-admission modes (Follow-up Task 3) and migrated policies (Follow-up Task 2) continue to function correctly using the reduced fast-evaluator output; no policy or admission mode silently loses required evidence.
- [ ] #7 Existing evaluator-snapshot-isolation and config-inspector Nix checks continue to pass after the reduction.
<!-- AC:END -->

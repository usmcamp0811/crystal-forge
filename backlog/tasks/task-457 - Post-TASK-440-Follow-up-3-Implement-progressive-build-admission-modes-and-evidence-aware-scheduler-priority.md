---
id: TASK-457
title: >-
  Post-TASK-440 Follow-up 3: Implement progressive build admission modes and
  evidence-aware scheduler priority
status: To Do
assignee: []
created_date: '2026-09-06 23:07'
labels: []
dependencies:
  - TASK-440
documentation:
  - >-
    backlog/docs/doc-24 -
    Crystal-Forge-Evaluation-Evidence-Build-Admission-and-Deployment-Gating-Architecture.md
priority: high
type: feature
ordinal: 466000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

Today build admission is effectively all-or-nothing relative to fast-evaluation success; there is no explicit operator-selectable mode governing whether builds start before configuration policy evidence is available, and no evidence-aware scheduler priority promotion/demotion. This is "Follow-up Task 3 — Progressive build admission and scheduler priority" from the "Crystal Forge Evaluation, Evidence, Build Admission, and Deployment Gating Architecture" design document (doc-24, §10-12, §21).

This task depends on the canonical evidence/decision models produced by the sibling Follow-up Task 1 (define canonical evaluation evidence architecture). Coordinate sequencing with that task; do not invent a divergent ad hoc admission-state model if Follow-up Task 1 has already defined one.

## Non-goals

- Do not migrate individual policy types to Config-artifact-backed evidence in this task (that is Follow-up Task 2); this task consumes whatever policy evidence already exists.
- Do not remove existing derivation policy fields (`cf_agent_enabled`, `policy_requirements_met`, `policy_results`) unless the evidence/migration plan from Follow-up Task 1 explicitly authorizes it.
- Do not reduce or narrow the primary fast evaluator in this task (that is Follow-up Task 4).
- Do not implement automatic cancellation of already-running builds on later policy failure; per design §9, that remains an explicit, separately-scoped future scheduler/resource policy.

## Scope

1. Implement three explicit build-admission modes per design §10: Build-all, CF-enabled (recommended default), and Policy-gated.
2. Represent build eligibility using explicit persisted, auditable states/reasons (e.g. `held: cf_agent_disabled`, `held: config_evidence_pending`, `eligible: policy_verified`, `blocked_deploy: strict_config_policy_failed`) rather than a single boolean, per design §11.
3. Implement evidence-aware scheduler priority promotion/demotion so that stronger policy evidence promotes queued work and a known strict pre-build policy failure holds/deprioritizes not-yet-started work, per design §12. Already-running builds are not cancelled by this change.
4. Ensure deep Config inspection and build/cache execution can run concurrently once a build is admitted, per design §5 and §17 (parallelism requirement); do not serialize deep inspection behind build completion or vice versa.
5. Add UI configuration for selecting the build-admission mode and UI status display of admission state/reason per configuration, consistent with the existing web-ui patterns.
6. Preserve existing deployment-policy gating: build-admission mode must never bypass deployment-time policy checks (design §9 — build admission and deployment authorization remain separate questions).
7. Fail closed / hold (not silently pass) when required evidence for the selected mode is missing, per design §10 (Policy-gated: "missing evidence is an explicit pending/held state, not an implicit pass").

## Verification plan

- `nix develop -c cargo check --package cf-server` and targeted `cargo test` coverage for admission-state transitions in each of the three modes, including missing-evidence and policy-failure hold/promote paths.
- Targeted server tests proving: (a) Build-all admits immediately after Tier-1 derivation discovery regardless of policy evidence; (b) CF-enabled holds until `cfAgentEnabled=true` and promotes/holds queued work based on later policy evidence without cancelling running builds; (c) Policy-gated holds until required pre-build policy evidence passes and treats missing evidence as pending, not passing.
- Update SQLx offline metadata if admission-state persistence requires schema/query changes; add a migration for any new persisted state.
- Authoritative `web-ui` check coverage for the new mode-selection UI and admission-state/reason display, including light/dark and narrow-viewport states per existing web-ui verification conventions.
- Confirm no regression to existing deployment-policy gating tests.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Build-all mode admits a build immediately after Tier-1 (fast evaluator) produces a valid derivation, without waiting on any config-derived policy evidence, while deep Config inspection still runs in parallel.
- [ ] #2 CF-enabled mode holds build admission until Tier-1 proves cfAgentEnabled=true, then admits the build; deep policy evaluation runs concurrently with the build and does not block it once admitted.
- [ ] #3 Policy-gated mode holds the build until all configured pre-build policies have sufficient evidence and pass; missing evidence produces an explicit pending/held state rather than an implicit pass.
- [ ] #4 Build eligibility is represented by explicit persisted, auditable states/reasons (not a single opaque boolean), covering at minimum held/eligible/priority/blocked_deploy classes from design document §11.
- [ ] #5 Scheduler priority is promoted for queued work when stronger policy evidence becomes available, and held/deprioritized when a known strict pre-build policy failure occurs, without automatically cancelling an already-running build.
- [ ] #6 Deployment authorization remains gated by existing deployment policy checks regardless of the selected build-admission mode; no mode bypasses deployment-time gating.
- [ ] #7 The UI exposes an explicit, persisted build-admission mode selection and displays the current admission state/reason for a configuration.
- [ ] #8 Targeted server tests cover all three modes' admission, hold, promotion, and missing-evidence behavior, and pass under nix develop -c cargo test for the affected package.
- [ ] #9 The authoritative web-ui check passes for the new mode-selection and admission-state UI, including light/dark theme and narrow-viewport coverage.
<!-- AC:END -->

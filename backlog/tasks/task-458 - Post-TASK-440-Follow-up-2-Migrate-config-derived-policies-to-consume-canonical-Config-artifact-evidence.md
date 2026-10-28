---
id: TASK-458
title: >-
  Post-TASK-440 Follow-up 2: Migrate config-derived policies to consume
  canonical Config artifact evidence
status: To Do
assignee: []
created_date: '2026-09-06 23:08'
labels: []
dependencies:
  - TASK-440
  - TASK-456
documentation:
  - >-
    backlog/docs/doc-24 -
    Crystal-Forge-Evaluation-Evidence-Build-Admission-and-Deployment-Gating-Architecture.md
priority: high
type: feature
ordinal: 467000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

Deployment policies today independently re-evaluate configuration facts via Nix (e.g. `cfAgentEnabled`, `require_packages`, `nixos_option` assertions, custom config expressions) rather than consuming the immutable, already-computed `ConfigArtifactV2` evidence produced by the TASK-440 Config Inspector. This duplicates evaluation work and creates multiple partially-overlapping sources of truth. This is "Follow-up Task 2 — Move config-derived policies to Config artifacts" from the "Crystal Forge Evaluation, Evidence, Build Admission, and Deployment Gating Architecture" design document (doc-24, §7, §21).

This task depends on the evidence identity/version contract and `PolicyAssessment` model defined by the sibling Follow-up Task 1 (TASK-456). Use that contract; do not invent a divergent evidence-binding scheme.

## Non-goals

- Do not eliminate arbitrary Nix `custom_check` policy expressions; per design §7, arbitrary Nix evaluation remains an explicit, isolated escape hatch for policy logic that cannot be represented against typed evidence.
- Do not implement build-admission modes or scheduler priority in this task (Follow-up Task 3); this task only changes what evidence source config-derived policies read from.
- Do not reduce or narrow the primary fast evaluator in this task (Follow-up Task 4) beyond what is a direct, necessary consequence of a specific policy no longer needing a duplicate Nix evaluation.
- Do not change deployment-policy authorization semantics (pass/fail/waived outcomes) for a migrated policy type; the evidence source changes, the policy decision contract does not.

## Scope

Migrate the following policy/assertion types, starting with the set identified in design §7 and §21, to consume `ConfigArtifactV2` evidence instead of independently evaluating Nix:

1. `require_cf_agent` — use the Config artifact as canonical evidence (the fast-evaluator fact may remain for early build admission per Follow-up Task 3, but the deployment-time policy assessment consumes Config-artifact evidence).
2. `require_packages` over `environment.systemPackages` — read from Config artifact package evidence.
3. Typed `nixos_option` assertions — read effective option values/types from the Config artifact rather than a separate Nix evaluation.
4. Representable custom `config.foo.bar == literal`-style config assertions — read from the Config artifact when the assertion is expressible against typed evidence.
5. Compatible composite config assertions — migrate where the composite is fully expressible against Config artifact evidence.

For each migrated policy type:
- Bind the policy assessment to the exact immutable Config artifact identity and exact policy-set/version, per design §13 and §20.
- Preserve existing pass/fail/waived policy outcomes and audit trail for currently deployed configurations; do not silently change historical policy results.
- Handle progressive evidence readiness per design §8: Stage-A (policy-ready) facts are sufficient for these migrated policy types; do not block policy assessment on Stage-B (rich provenance/audit) enrichment.
- Handle missing/unavailable Config artifact evidence as an explicit pending/unavailable policy state (fail-closed for deployment), not a fabricated pass or fail, per design §19-20.

## Verification plan

- `nix develop -c cargo check --package cf-server` and targeted `cargo test` coverage per migrated policy type, including: matching outcome parity against the prior Nix-evaluation-backed result for representative fixtures; missing-Config-artifact-evidence pending/fail-closed behavior; exact evidence-identity/policy-set-version binding on the resulting PolicyAssessment.
- Update SQLx offline metadata for any changed policy-assessment persistence/query shapes.
- Regression coverage confirming policies not in this task's migration list (e.g. CVE thresholds, approvals, time windows, canary rollout, arbitrary `custom_check`) are unaffected.
- Confirm no fast-evaluator or build-admission behavior change beyond what Follow-up Task 1's plan explicitly authorizes.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 require_cf_agent deployment-time policy assessment reads canonical evidence from the ConfigArtifactV2 rather than independently re-evaluating Nix.
- [ ] #2 require_packages assertions over environment.systemPackages read package evidence from the Config artifact rather than independently re-evaluating Nix.
- [ ] #3 Typed nixos_option assertions read effective option values/types from the Config artifact rather than independently re-evaluating Nix.
- [ ] #4 Representable simple and compatible composite config assertions expressible against typed Config artifact evidence are migrated to read from that evidence; assertions not expressible against typed evidence continue to use the arbitrary Nix escape hatch unchanged.
- [ ] #5 Each migrated policy assessment binds to the exact immutable Config artifact identity and exact policy-set/version, and this binding is inspectable/auditable.
- [ ] #6 Migrated policy types produce pass/fail/waived outcomes consistent with their prior Nix-evaluation-backed behavior for representative fixtures; no fabricated pass or fail occurs when required Config artifact evidence is missing or unavailable, which instead yields an explicit pending/unavailable state that fails closed for deployment.
- [ ] #7 Migrated policy assessment does not block on Stage-B rich provenance/audit enrichment when the policy only requires Stage-A policy-ready facts.
- [ ] #8 Policy types intentionally out of scope for this task (CVE thresholds, approvals, time windows, canary rollout, arbitrary custom_check) are unaffected.
- [ ] #9 Targeted server tests for each migrated policy type pass under nix develop -c cargo test, and SQLx offline metadata is updated for any changed persistence/query shapes.
<!-- AC:END -->

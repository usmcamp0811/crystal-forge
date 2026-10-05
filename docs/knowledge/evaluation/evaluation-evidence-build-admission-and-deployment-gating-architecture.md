---
type: Design Specification
title: "Evaluation, evidence, build admission, and deployment gating architecture (doc-24)"
description: "Pointer to the retained Backlog document doc-24: the proposed post-TASK-440 three-tier evaluation model, build admission modes, PolicyAssessment as decision source, policy phases, performance requirements, and follow-up tasks."
tags:
  - crystal-forge
  - evaluation
  - policy
  - build-admission
  - deployment-gating
implementation_status: proposed
generated:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-03T22:58:30-05:00
sources:
  - id: origin
    resource: "Crystal Forge repository file backlog/docs/doc-24%20-%20Crystal-Forge-Evaluation-Evidence-Build-Admission-and-Deployment-Gating-Architecture.md at commit 3b23d36f"
    title: "Crystal Forge Evaluation, Evidence, Build Admission, and Deployment Gating Architecture"
---

# Evaluation, evidence, build admission, and deployment gating architecture (doc-24)

This concept is a navigation and status record. The retained Backlog document is the authoritative text. Backlog.md manages it by ID, so it stays at its original path:
[doc-24 - Crystal Forge Evaluation, Evidence, Build Admission, and Deployment Gating Architecture](<../../../backlog/docs/doc-24 - Crystal-Forge-Evaluation-Evidence-Build-Admission-and-Deployment-Gating-Architecture.md>).

## What the document specifies

The document is a design proposal (created 2026-09-06) for follow-up work after TASK-440. Its stated status is "Design proposal / post-TASK-440 follow-up". Its objective is to keep the fast evaluation and drift-detection loop while removing duplicated configuration and policy evaluation, and to use build time as latency budget for deeper inspection. It has 25 numbered sections:

1. **Problem statement**: the primary `nix-eval-jobs` evaluator, deployment policies, the TASK-440 Config Inspector, and Flake Explorer evaluate overlapping facts, and `cf_agent_enabled`, `policy_requirements_met`, and `policy_results` carry overlapping meaning.
2. **User intent and product behavior**: fast eval, immediate drift classification, early build admission once the agent is proven enabled, deep Config inspection in parallel with the build, and a deployment gate that needs both build readiness and policy evidence.
3. **Core principle: preserve the narrow fast evaluator**, with explicit fast-evaluator responsibilities and non-responsibilities (no `cfg.options` traversal, `_module.graph`, full provenance, general custom policy expressions, or Flake Explorer enumeration).
4. **Three-tier evaluation model**: Tier 1 fast identity evaluation, Tier 2 deep per-configuration Config inspection (`ConfigArtifactV2`), Tier 3 evidence-driven policy assessment.
5. **Parallel pipeline** and the critical-path rule `time_to_deployment = fast_eval + max(build_and_cache, config_inspection_and_policy) + deployment_runtime_gates`.
6. **Drift detection must stay fast**: store-path equality against the running path, independent of deep inspection and build completion.
7. **ConfigArtifactV2 becomes canonical configuration evidence**, with a candidate mapping from policy types to evidence sources and arbitrary `custom_check` Nix kept as an isolated escape hatch.
8. **Progressive Config evidence**: Stage A policy-ready facts and Stage B rich audit enrichment.
9. **Build admission is separate from deployment authorization**, and a running build is not cancelled by default when a later policy fails.
10. **Build admission modes**: Mode A Build all, Mode B CF-enabled (recommended default candidate), and Mode C Policy-gated.
11. **Progressive build-admission state machine** with persisted, visible decision reasons.
12. **Build prioritization** as a dimension separate from admission.
13. **`PolicyAssessment` becomes the decision source of truth**, reproducible from immutable evidence artifacts and an exact policy-set version.
14. **Policy phases**: pre-build, build-priority, and pre-deploy.
15. **Flake Explorer relationship**: shared artifact and job infrastructure, separate semantic artifact types.
16. **Isolation requirements**: per-configuration inspection and failure isolation.
17. **Performance requirements and SLOs** (ten goals) and 18. **Example timing**.
19. **Failure semantics** for fast evaluation failure, disabled CF agent, unavailable deep inspection, late policy failure, and Stage-B failure.
20. **Security and integrity requirements**.
21. **Migration strategy after TASK-440** and five follow-up tasks (canonical evidence architecture, move config-derived policies to Config artifacts, progressive build admission and scheduler priority, reduce the primary evaluator, unify artifact and job infrastructure).
22. **Acceptance criteria** (18 unchecked items), 23. **Non-goals**, 24. **Design invariants** (14), and 25. **Short version**.

## Implementation status

Status: **proposed**, with the prerequisites partly in place. Evidence checked on the migration branch:

- Tier 2 prerequisites exist. A separate Config Inspector worker and V2 snapshot artifact exist (`packages/default/crates/cf-server/src/services/config_inspections.rs`, `packages/default/crates/cf-server/src/models/config_snapshot_artifact.rs`). They are described in [evaluation-flake-snapshot-architecture.md](evaluation-flake-snapshot-architecture.md) and [config-explorer-implementation-status.md](config-explorer-implementation-status.md). The document's name `ConfigArtifactV2` does not appear in code.
- The narrow primary evaluator exists. `packages/default/crates/cf-server/src/models/primary_evaluation.nix` evaluates `cfAgentEnabled`, and [config-explorer-architecture.md](config-explorer-architecture.md) records the same boundary.
- Build admission is currently a fixed rule, not a mode. `packages/default/crates/cf-server/src/queries/build_jobs.rs` filters on `d.cf_agent_enabled = TRUE`. No Build-all, CF-enabled, or Policy-gated mode setting was found.
- `PolicyAssessment`, policy phases (`pre-build`, `build-priority`, `pre-deploy`), and persisted build-admission decision reasons were not found. `derivations.policy_requirements_met` and `policy_results` remain in use (`packages/default/crates/cf-server/src/queries/derivations.rs`).
- Config inspection currently enqueues after primary evaluation succeeds, as [evaluation-flake-snapshot-architecture.md](evaluation-flake-snapshot-architecture.md) states. Whether it runs concurrently with the build, as this document requires, was not verified.
- Arbitrary `custom_check` expressions exist (`packages/default/crates/cf-server/src/models/custom_check.rs`).

## Related concepts

* [Config Explorer Architecture](config-explorer-architecture.md) - The accepted TASK-440 design that this proposal builds on.
* [Evaluation and Flake Snapshot Architecture](evaluation-flake-snapshot-architecture.md) - Ownership of snapshots and the Config Inspector job.
* [Config Explorer decision record](../decisions/config-explorer-decisions.md) - Decisions that this proposal must not weaken.

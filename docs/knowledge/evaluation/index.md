# Architecture

* [Config Explorer current implementation map](config-explorer-implementation-status.md) - Maps the Config Explorer design to its implementing server, worker, Nix expression, query, migration, API, and Web UI paths, and describes how scoped observations, V2 snapshot reuse, and paged root and prefix observations currently work.

# Operator Guide

* [Bulk evaluator memory planning and timeouts](bulk-evaluator-resource-planning.md) - Open when sizing bulk commit evaluation or upgrading its memory and timeout defaults; explains automatic limits, explicit per-worker overrides, worker resolution, and pinned upstream threshold semantics.

# Concept

* [SystemD-Run Evaluation Isolation in Crystal Forge](systemd-run-evaluation-isolation.md) - Explains the systemd-run scope isolation (memory, CPU, timeout limits) and the direct-execution fallback that keep Nix evaluations from OOM-killing the server, with trade-offs, failure modes, and proposed improvements.

# Design Specification

* [Config Explorer Architecture](config-explorer-architecture.md) - Specifies the Config Explorer design: why full option crawls are the wrong prerequisite, the three-evaluator authority invariants, the phased lazy inspection model, the Configured options classifier, and the benchmark record.
* [Config Explorer failure containment, scheduling, security, and API principles](config-explorer-resource-security-and-api-model.md) - Covers Config Explorer failure containment, resource scheduling priority and capacity states, request lifecycle, security rules, process and timeout model, optional complete inventory, API principles, data flow, non-goals, and future evolution.
* [Config Explorer target identity, cache contract, and snapshot semantics](config-explorer-target-identity-and-snapshot-semantics.md) - Specifies upgraded-fleet current revision recovery, the immutable target identity and cache contract, the split between Explorer observations and certified V2 snapshots, and the Changed, Drift, and search semantics.
* [Evaluation and Flake Snapshot Architecture](evaluation-flake-snapshot-architecture.md) - Defines ownership and data flow for revision-specific evaluation and flake-output snapshots: what PRIMARY and the Config Inspector worker own, immutable artifacts versus the mutable selector, deployment binding, and database-only reads.
* [Evaluation snapshot identity, comparison, and lifecycle](evaluation-snapshot-identity-lifecycle-and-comparison.md) - Specifies full-SHA snapshot identity, first-parent resolution and Changed comparison rules, and the queued/running/failed/available/unavailable lifecycle for Config snapshot reads and the targeted Config inspection mutation.
* [Evaluation snapshot persistence, bounds, reclamation, and redaction](evaluation-snapshot-persistence-bounds-and-redaction.md) - Describes content-addressed snapshot persistence, advisory-lock ordering, hard size bounds, the Stage 2 indexed membership cost, orphan reclamation, and the safe-value and redaction policy applied before persistence.
* [Evaluation snapshot retention, generation rollback, and source reset](evaluation-snapshot-retention-and-rollback.md) - Explains how retained deployment generations keep snapshots, derivations, and commits alive, how generation rollback resolves exact lineage, how branch rewrite and source reset archive commits, and how unavailable content is reported.
* [Evaluation, evidence, build admission, and deployment gating architecture (doc-24)](evaluation-evidence-build-admission-and-deployment-gating-architecture.md) - Pointer to the retained Backlog document doc-24: the proposed post-TASK-440 three-tier evaluation model, build admission modes, PolicyAssessment as decision source, policy phases, performance requirements, and follow-up tasks.
* [Flake outputs, system reconciliation, and count authority](flake-outputs-and-count-authority.md) - Defines the PRIMARY flake-output projection, managed/declared_unmanaged/managed_undeclared reconciliation, which counts are authoritative, Systems and Inputs pane filters, and non-disclosure rules for hidden environments.
* [NixOS Option Metadata Authority](nixos-option-metadata-authority.md) - States the invariant that Crystal Forge's packaged NixOS option metadata is only an authoring aid derived from its pinned nixpkgs, never authoritative for a monitored flake, and defines the unknown/custom fallback and Phase 3/4 separation.

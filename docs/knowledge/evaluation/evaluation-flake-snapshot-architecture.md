---
type: Design Specification
title: "Evaluation and Flake Snapshot Architecture"
description: "Defines ownership and data flow for revision-specific evaluation and flake-output snapshots: what PRIMARY and the Config Inspector worker own, immutable artifacts versus the mutable selector, deployment binding, and database-only reads."
tags:
  - crystal-forge
  - evaluation
  - snapshot
  - config-inspector
  - data-flow
implementation_status: implemented
generated:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-03T22:57:42-05:00
sources:
  - id: origin
    resource: "Crystal Forge repository file docs/evaluation-flake-snapshots.md at commit 3b23d36f"
    title: "Evaluation and Flake Snapshot Architecture"
---

# Evaluation and Flake Snapshot Architecture

## Purpose

Crystal Forge stores reusable, revision-specific read models for the System
Config view and the flake explorer. These read models expose evaluated NixOS
options and flake outputs without running Nix, Git, or network operations from
an HTTP read request.

This document defines the ownership, identity, security, lifecycle, retention,
and API contracts for those snapshots.

## Ownership and Data Flow

PRIMARY owns system derivation and policy evaluation. It also emits one
revision-scoped flake-output projection. PRIMARY MUST NOT inspect per-host option
trees or module graphs. After PRIMARY succeeds, an exact commit and configuration
can have one durable Config Inspector job. The separate Config Inspector worker
reuses the evaluated carrier derivation and persists a V2 artifact without
changing build or deployment eligibility. Unsupported or failed inspection and
an unreadable option-tree root remain explicitly unavailable. Unreadable
non-root prefixes produce a partial artifact that retains healthy sibling
options. The worker does not fabricate an empty available snapshot.

Snapshot persistence redacts metadata before storage. The server does not
serialize the NixOS `config` tree. Missing exploration artifacts do not change
system evaluation, policy, build, or deployment state.

Migration `0248_immutable_evaluation_artifacts.sql` separates immutable attempt
artifacts from the mutable current selector for each exact commit and
configuration. Each success or failure inserts a new artifact and advances the
selector atomically. A retained generation points to the exact successful
artifact and derivation that produced it. A later success or failure does not
rewrite that retained artifact.

Pre-0248 retained metadata remains queryable after upgrade. Migration validates
the complete copied artifact and marks valid content readable. The mutable
legacy schema cannot prove exact deployment/store lineage, so migration marks
these rows unverified. This flag does not affect Config validity or comparison;
it makes the generation ineligible for rollback only.

When a deployment request binds its full commit and exact derivation target, it
also captures the current successful evaluation artifact. Generation retention
uses this captured identity. It does not consult the mutable selector when the
agent reports the generation later. Reciprocal retention considers only
observations timestamped at or after the bound deployment was issued. Commit
rollback uses the resolved commit's artifact. Generation rollback carries the
retained artifact into the new
deployment instead of selecting the current attempt for the commit. If the
deployment, derivation, store path,
configuration, and artifact lineage cannot be matched exactly, retention fails
closed and the generation is not advertised as rollback-eligible.
Migration marks pre-0248 deployments as not expecting an artifact binding.
Only deployments created after migration participate in reciprocal binding.
Unavailable artifacts, including snapshots over the content limit, advance the
current selector but do not bind deployments or create generation retention.
Pending and succeeded deployments can create retention. An expired deployment
can also create retention until 24 hours after its terminal completion. The row
remains `expired`; a delayed successful activation creates a correlated
`cf_deployment_succeeded` event instead of rewriting terminal status. Failed and
superseded deployments cannot create retention or successful-activation
correlation. The server selects the newest same-path deployment issued no later
than the observation before it checks eligibility. It does not skip a newer
failed or superseded request to attach the observation to older work.

Migration `0247_authoritative_snapshot_metrics.sql` performs the historical
host-delta backfill. In normal operation,
the application persists the complete configuration corpus with deferred
per-row recomputation, then recomputes commit-wide host deltas once before the
finalization transaction becomes visible. Replacement and failure paths use
the same transaction-level recomputation contract.

For an available configuration, `module_count` is the exact count of distinct
`(source_input, source_revision, source_path)` tuples in the persisted option
definitions. The server computes this scalar after redaction and per-option
bounding. Response-only tracked identities do not affect it. Existing snapshots
are backfilled with the same tuple semantics.

Migration `0254_partial_config_option_inventories.sql` marks existing certified
schema-V2 snapshots as complete with empty, non-truncated diagnostics. Before
0254, Stage 1 constructed the complete global option index before persistence,
and reconciliation required every indexed metadata and value result. A
successful certified V2 snapshot therefore could not represent a partial
inventory. Schema-V1 rows retain null inventory fields and no V2 completeness
claim. The migration does not infer or destructively rewrite option content.

Snapshot GET handlers and query functions are database-only. They MUST NOT
invoke Nix, inspect Git, fetch a repository, enqueue work, or perform per-host
evaluation. A missing snapshot remains a read result. The explicit targeted
Config inspection mutation can queue or reuse only an exact Config Inspector
job after primary evaluation has persisted its carrier.

This design does not add an agent or builder protocol field. Deployed agents
continue to report state and generations through the existing protocol.
Deployed builders continue to use the existing server-issued job authorization
and evaluation path. Targeted Config inspection remains server-owned work, not a
new database or API responsibility for an API-only builder.

## Related concepts

* [Evaluation snapshot identity, comparison, and lifecycle](evaluation-snapshot-identity-lifecycle-and-comparison.md) - Specifies full-SHA snapshot identity, first-parent resolution and Changed comparison rules, and the queued/running/failed/available/unavailable lifecycle for Config snapshot reads and the targeted Config inspection mutation.
* [Evaluation snapshot persistence, bounds, reclamation, and redaction](evaluation-snapshot-persistence-bounds-and-redaction.md) - Describes content-addressed snapshot persistence, advisory-lock ordering, hard size bounds, the Stage 2 indexed membership cost, orphan reclamation, and the safe-value and redaction policy applied before persistence.
* [Evaluation snapshot retention, generation rollback, and source reset](evaluation-snapshot-retention-and-rollback.md) - Explains how retained deployment generations keep snapshots, derivations, and commits alive, how generation rollback resolves exact lineage, how branch rewrite and source reset archive commits, and how unavailable content is reported.
* [Flake outputs, system reconciliation, and count authority](flake-outputs-and-count-authority.md) - Defines the PRIMARY flake-output projection, managed/declared_unmanaged/managed_undeclared reconciliation, which counts are authoritative, Systems and Inputs pane filters, and non-disclosure rules for hidden environments.
* [Evaluation snapshot API and URL state](../api/evaluation-snapshot-api-and-url-state.md) - Describes server-side option search, filter, pagination and snapshot tokens (409 snapshot_changed), flake output paging, module declaration and module-source endpoints, summary field meanings, tracked provenance, and System Detail URL state.
* [Config Explorer Architecture](config-explorer-architecture.md) - Specifies the Config Explorer design: why full option crawls are the wrong prerequisite, the three-evaluator authority invariants, the phased lazy inspection model, the Configured options classifier, and the benchmark record.

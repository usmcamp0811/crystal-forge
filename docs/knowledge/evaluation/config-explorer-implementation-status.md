---
type: Architecture
title: "Config Explorer current implementation map"
description: "Maps the Config Explorer design to its implementing server, worker, Nix expression, query, migration, API, and Web UI paths, and describes how scoped observations, V2 snapshot reuse, and paged root and prefix observations currently work."
tags:
  - crystal-forge
  - config-explorer
  - evaluation
  - implementation-status
  - task-440
implementation_status: implemented
generated:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-03T22:57:42-05:00
sources:
  - id: origin
    resource: "Crystal Forge repository file docs/config-explorer-architecture.md at commit 3b23d36f"
    title: "Config Explorer Architecture"
---

# Config Explorer current implementation map

## Current implementation

The current TASK-440 implementation is distributed across these boundaries:

- Explorer service and worker: `packages/default/crates/cf-server/src/services/config_inspections.rs`,
  `packages/default/crates/cf-server/src/services/config_observations.rs`,
  `packages/default/crates/cf-server/src/bin/config-inspector-worker.rs`, and
  `packages/default/crates/cf-server/src/models/config_inspector.rs`.
- Trusted inspector expressions:
  `packages/default/crates/cf-server/src/models/config_inspector.nix`,
  `packages/default/crates/cf-server/src/models/config_shallow_observer.nix`,
  `packages/default/crates/cf-server/src/models/config_observer.nix`, and
  `packages/default/crates/cf-server/src/models/config_value_encoding.nix`.
- Explorer queries and persistence:
  `packages/default/crates/cf-server/src/queries/config_inspections.rs`,
  `packages/default/crates/cf-server/src/queries/config_observations.rs`, and
  `packages/default/crates/cf-server/src/models/config_observations.rs`, and
  `packages/default/crates/cf-server/src/security/snapshot_redaction.rs`.
- Evaluation snapshot queries and V2 model:
  `packages/default/crates/cf-server/src/queries/evaluation_snapshots.rs`,
  `packages/default/crates/cf-server/src/models/evaluation_snapshots.rs`, and
  `packages/default/crates/cf-server/src/models/config_snapshot_artifact.rs`.
- Snapshot and inspection schema:
  `packages/default/crates/cf-server/migrations/0245_evaluation_and_flake_output_snapshots.sql`,
  `0248_immutable_evaluation_artifacts.sql`,
  `0249_snapshot_capture_diagnostics.sql`,
  `0250_config_snapshot_artifact_v2.sql`,
  `0252_config_inspection_jobs.sql`,
  `0253_config_inspection_execution_ownership.sql`,
  `0254_partial_config_option_inventories.sql`, and
  `0255_scoped_config_observations.sql`,
  `0256_config_observation_child_pages.sql`,
  `0266_version_source_bound_config_observations.sql`, and
  `0267_version_source_bound_config_observation_requests.sql`.
- API handlers and models: the Config inspection handlers and API models under
  `packages/default/crates/cf-server/src/handlers/api/` and
  `packages/default/crates/cf-server/src/api/models.rs`.
- Web UI Config view: the Config/system-detail surfaces under
  `packages/web-ui/src/views/` and `packages/web-ui/src/components/`.
- Deliberately separate policy evaluator path:
  `packages/default/crates/cf-server/src/models/evaluate_with_policies.rs`,
  `packages/default/crates/cf-server/src/deployment/mod.rs`, and the policy
  services under `packages/default/crates/cf-server/src/services/`.

These paths identify ownership. They do not authorize a future change to make
Explorer data authoritative for policy or deployment.

The request path first reuses an exact scoped observation. A complete certified
V2 artifact can then answer root or prefix pages and exact option or provenance
reads when its commit, configuration, target key, and carrier all match. A
partial V2 artifact can answer only an individually present exact option or its
surviving-definition provenance. Partial V2 never establishes missing tree
children. Configured-index requests continue to use the dedicated classifier
because V2 does not preserve the required default-versus-configuration proof.
V2 reuse writes only a normal observational cache entry and does not change a
snapshot selector.

Root and prefix observations use deterministic server-bounded pages of at most
512 immediate children. The zero-based child offset is part of the request and
cache identity. The UI can load subsequent pages within the server's bounded
offset range without accepting Nix source from the browser. If evaluation of an
option value or nested encoded value fails, the
option observation retains readable metadata and returns a bounded
`value_unavailable` value state without a raw Nix trace.

Uncached shallow observations materialize the commit's verified source archive
and evaluate only the requested bounded root, prefix, option, or provenance
shape. Schema version 2 separates these immutable-source results from legacy
carrier evaluation. Content digests include the schema domain so identical JSON
from different interpretation contracts cannot share one digest identity.

## Related concepts

* [Config Explorer Architecture](config-explorer-architecture.md) - Specifies the Config Explorer design: why full option crawls are the wrong prerequisite, the three-evaluator authority invariants, the phased lazy inspection model, the Configured options classifier, and the benchmark record.
* [Config Explorer decision record](../decisions/config-explorer-decisions.md) - Records the ten accepted Config Explorer decisions (observational only, lazy scoped browsing, optional V2 snapshots, exact-identity caching, no client Nix expressions) and the rule that violating work must amend the architecture.
* [TASK-440 Config Explorer design audit](../historical/task-440-config-explorer-design-audit.md) - Records the TASK-440 design audit of the Config side column and Flake Modules pane against the reference design, the intentional differences, geometry-sensitive Config pagination, and the screenshot evidence policy.

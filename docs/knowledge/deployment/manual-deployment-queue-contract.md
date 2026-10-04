---
type: Design Specification
title: "Manual deployment queue contract"
description: "Specifies the manual deployment actions (deploy, continue_auto_latest, convert_to_manual), partial-success semantics, and the request_id idempotency and 24-hour legacy replay rules."
tags:
  - crystal-forge
  - deployment
  - queue
  - idempotency
implementation_status: implemented
generated:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-03T22:57:42-05:00
sources:
  - id: origin
    resource: "Crystal Forge repository file docs/evaluation-flake-snapshots.md at commit 3b23d36f"
    title: "Evaluation and Flake Snapshot Architecture"
---

# Manual deployment queue contract

This contract was part of the evaluation and flake snapshot architecture document.

## Deployment Queue Contract

Manual deployment accepts `deploy`, `continue_auto_latest`, and
`convert_to_manual`. An `auto_latest` system requires an explicit choice.
Conversion to manual commits independently before deployment queueing. A later
queue failure therefore returns partial success: the response reports the
persisted manual policy, the conversion result, and a failed deployment state.
A conversion failure queues no deployment.

New clients send a UUID `request_id` and reuse it until the request reaches its
reported result. The server reserves that immutable system, full commit SHA,
and action before conversion. Matching retries reuse partial state or the
deployment ID. Reusing the UUID for another intent returns conflict before
policy mutation. Legacy clients that omit `request_id` receive a stable derived
identity with a 24-hour replay window; after that window the same target can be
deployed intentionally again. Queueing serializes on the system row and reuses
matching pending work.

## Related concepts

* [Evaluation and Flake Snapshot Architecture](../evaluation/evaluation-flake-snapshot-architecture.md) - Defines ownership and data flow for revision-specific evaluation and flake-output snapshots: what PRIMARY and the Config Inspector worker own, immutable artifacts versus the mutable selector, deployment binding, and database-only reads.
* [Evaluation snapshot retention, generation rollback, and source reset](../evaluation/evaluation-snapshot-retention-and-rollback.md) - Explains how retained deployment generations keep snapshots, derivations, and commits alive, how generation rollback resolves exact lineage, how branch rewrite and source reset archive commits, and how unavailable content is reported.

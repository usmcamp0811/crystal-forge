---
type: Design Specification
title: "Remote builder architecture status and follow-up plan (doc-16)"
description: "Pointer to the retained Backlog document doc-16: TASK-375 status of the API-only remote builder, the derivation-transport problem, the server_derivation and source_re_evaluate_verified strategies, the proposed attempt phases, and design principles."
tags:
  - crystal-forge
  - builder
  - remote-builds
  - task-375
implementation_status: partial
generated:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-03T22:58:30-05:00
sources:
  - id: origin
    resource: "Crystal Forge repository file backlog/docs/builders/remote-builder-architecture-status/doc-16%20-%20Remote-Builder-Architecture-Status-and-Follow-up-Plan.md at commit 3b23d36f"
    title: "Remote Builder Architecture Status and Follow-up Plan"
---

# Remote builder architecture status and follow-up plan (doc-16)

This concept is a navigation and status record. The retained Backlog document is the authoritative text. Backlog.md manages it by ID, so it stays at its original path:
[doc-16 - Remote Builder Architecture Status and Follow-up Plan](<../../../backlog/docs/builders/remote-builder-architecture-status/doc-16 - Remote-Builder-Architecture-Status-and-Follow-up-Plan.md>).

## What the document specifies

The document is a status snapshot and plan written after TASK-375 (created 2026-06-30). Its sections are:

- **Current status after TASK-375**: the API-only remote builder path works for hotfix validation. Builders resolve identity through the server API, builder public keys persist through the Builders UI, jobs carry the server-evaluated derivation payload, and builders report logs, progress, completion, cancellation, and failure over HTTP and WebSocket APIs. The production model is `server_derivation`.
- **Problem discovered during TASK-375 validation**: transporting the server-evaluated derivation and its input closure to a remote builder that does not share the server `/nix/store` is the hard part. Temporary mechanisms (synchronous cache publication of the `.drv` closure, then a monolithic export archive) caused argument-size limits, multi-GiB archives, cache credentials in job bootstrap, and bootstrap failures that look like build failures.
- **Recommended direction**: keep `server_derivation` as the authoritative production default, replace monolithic archive transport with pull-based Nix store and substituter transport, and add an optional explicit `source_re_evaluate_verified` strategy. Never fall back silently between strategies. A fallback becomes a new explicit attempt chosen by scheduler policy.
- **Strategy overview**: a Mermaid flowchart from source revision and lock identity through server evaluation, expected derivation identity, policy binding, the execution strategy choice, the `derivation_mismatch` failure, and API reporting.
- **Follow-up tasks**: TASK-375.3 (pull-based store transport for `server_derivation`, with the conceptual substituter order of a Crystal Forge job-scoped endpoint, the organization Attic cache, `cache.nixos.org`, then other approved caches) and TASK-375.4 (the verified source re-evaluation strategy with its example manifest fields).
- **Strategy comparison**: a table comparing the two strategies on production default, authority, source credentials, transport pressure, evaluation duplication, evaluator sensitivity, policy binding, and best use.
- **Recommended attempt phases**: a Mermaid state diagram (`queued`, `assigned`, `acknowledged`, `materializing_inputs`, `verifying_derivation`, `building`, `uploading_outputs`, `finalizing`, `succeeded`, plus `failed`, `lost`, and `cancelled` transitions) and a list of useful error classes.
- **Design principles going forward**: API-only builders, server-side database access, `server_derivation` as default until another strategy is selected, no silent fallback inside an attempt, path-oriented streaming retryable transport, immutable source archives or tokens instead of broad Git credentials, recorded expected and actual derivation identities, and asynchronous durable cache publication.

## Implementation status

Status: **partial**. The document is a point-in-time plan, and the code has moved beyond it in some areas and not followed it in others.

Evidence in code (searched on the migration branch):

- `source_re_evaluate_verified`, `derivation_mismatch`, and the evaluator contract exist in `packages/default/crates/cf-protocol/src/builder.rs`, `packages/default/crates/cf-builder/src/bin/builder.rs`, and `packages/default/crates/cf-server/src/handlers/api/builders.rs`. The source strategy is documented in [verified-source-evaluator-contract.md](verified-source-evaluator-contract.md).
- Delta-aware derivation materialization exists. The server exposes `derivation-manifest` and `derivation-archive` routes (`packages/default/crates/cf-server/src/bin/server.rs`). See [remote-build-execution-strategies.md](remote-build-execution-strategies.md).
- `BuildFailurePhase` in `packages/default/crates/cf-protocol/src/builder.rs` defines the implemented failure phases (`source_fetch`, `source_identity_mismatch`, `source_input_availability`, `evaluator_incompatible`, `evaluation`, `derivation_mismatch`, `path_materialization`, `build`). See [builder-failure-phases-and-retry.md](builder-failure-phases-and-retry.md).
- The proposed fine-grained attempt states (`assigned`, `acknowledged`, `materializing_inputs`, `verifying_derivation`, `uploading_outputs`, `finalizing`, `lost`) were not found in the code. Build jobs use the `queued`, `building`, `success`, and `failed` states described in [builder-api-database-schema.md](../data-model/builder-api-database-schema.md).
- A job-scoped Crystal Forge store or substituter endpoint, as listed in the conceptual substituter order, was not found. The server instead streams exported archives from the manifest endpoints.
- The document names `server_derivation` as the production default. Later documents recommend `source_re_evaluate_verified` with `server_bundled_archive` for new deployments. This disagreement is recorded in [remote-build-execution-strategies.md](remote-build-execution-strategies.md) and is listed as a verification candidate in the migration report.

## Related concepts

* [Remote builder execution strategies](remote-build-execution-strategies.md) - Current strategy descriptions and the recommended default.
* [Verified-source evaluator contract](verified-source-evaluator-contract.md) - The implemented form of TASK-375.4.
* [Builder failure phases and retry strategy](builder-failure-phases-and-retry.md) - The implemented failure phases that replace the proposed error classes.
* [Builder trust boundaries and component definitions](builder-trust-boundaries-and-components.md) - Why builders stay API-only.

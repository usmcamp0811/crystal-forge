---
type: Decision
title: "ADR-000: Crystal Forge Architecture Overview"
description: "Records the accepted architecture decisions for Crystal Forge, the current status of each decision (including the ones the API-only builder and Dioxus UI superseded), and their successors; open it for the rationale behind Ed25519, Rust, flake-native design, and event-driven queues."
tags:
  - crystal-forge
  - decision
  - adr
  - architecture
implementation_status: implemented
generated:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-04T16:30:00-05:00
sources:
  - id: origin
    resource: "Crystal Forge repository file docs/architecture.md at commit 3b23d36f"
    title: "ADR-000: Crystal Forge Architecture Overview"
  - id: code-1
    resource: "Crystal Forge repository file packages/default/crates/cf-builder/src/builder/api_client.rs at commit 3b23d36f"
    title: API-only builder client
  - id: code-2
    resource: "Crystal Forge repository file packages/default/crates/cf-server/src/handlers/ui.rs at commit 3b23d36f"
    title: Embedded web UI
---

# ADR-000: Crystal Forge Architecture Overview

## Status

Accepted. Two of the original decisions were later superseded. The
[decision table](#current-status-of-each-decision) states which, with the
revision boundary `3b23d36f` and the successor concept for each.

## Context

Crystal Forge provides compliance monitoring and build coordination for NixOS
systems in regulated environments. The architecture must support cryptographic
verification, scalable build capacity, and integration with existing
compliance workflows.

## Decision

The component, data-flow, and queue descriptions that originally formed this
record are now maintained as separate concepts: [Core components](../components/core-components.md),
[Data flows](../architecture/data-flows.md),
[Wakeups and polling](../architecture/event-driven-queues.md), and
[Observability points](../operations/observability-and-troubleshooting.md).

### Current status of each decision

| # | Original decision | Status at `3b23d36f` | Successor |
| --- | --- | --- | --- |
| 1 | **Shared PostgreSQL** so servers and builders scale horizontally | **Superseded for builders.** Only the server connects to PostgreSQL. Builders are API-only, use signed and session-checked requests, and scale by registering more builders. This record makes no claim about multiple server processes. | [Core components](../components/core-components.md), [Builder architecture](../builders/builder-architecture-and-job-scheduling.md) |
| 2 | **Ed25519 signatures** verify agent communication | Current. Builder API requests use the same signing model. | [Builder request authentication](../security/builder-request-authentication-and-data-in-transit.md) |
| 3 | **Rust implementation** for memory safety | Current. | [Backend workspace](../architecture/backend-cargo-workspace.md) |
| 4 | **Event-driven queues**: immediate processing with coalesced wakeups and fallback polling | **Amended.** Evaluation wakeups are in-process with a fallback tick. Builds are discovered by API polling because the build wakeup has no waiter. | [Wakeups and polling](../architecture/event-driven-queues.md) |
| 5 | **Flake-native** integration with the Nix ecosystem | Current. | [Evaluation and flake snapshot architecture](../evaluation/evaluation-flake-snapshot-architecture.md) |
| 6 | **Isolated snapshot exploration**: PRIMARY emits the revision-scoped flake projection and does not inspect per-host option trees or module graphs. The durable Config Inspector worker runs separately, and inspection failure does not block builds or deployments. | Current. | [Config Explorer architecture](../evaluation/config-explorer-architecture.md) |
| 7 | **Option metadata authority**: packaged NixOS option metadata is an authoring baseline, and each target flake's evaluation remains authoritative. | Current for the packaged baseline. Target-specific metadata is a documented future enhancement. | [NixOS option metadata authority](../evaluation/nixos-option-metadata-authority.md) |

## Consequences

**Positive**

- Strong cryptographic security model for agent and builder requests.
- A memory-safe implementation reduces attack surface.
- One authoritative server owns persistence, policy, and evaluation, so builders
  need no database credentials.

**Negative**

- PostgreSQL is a single point of failure for the server. Standard high-availability
  practice mitigates this.
- Rust has a learning curve for contributors.

### Superseded consequences

The original record listed integration with Grafana as a positive consequence
and an initial dependency on Grafana for the user interface as a negative one.
The Dioxus web UI, served by the server and backed by the server API, replaced
that dependence. Grafana is not part of the current product architecture. The
optional `dashboards` module configuration and the Grafana integration test
remain in the repository as legacy surfaces. See the
[Ecosystem architecture summary](../architecture/ecosystem-architecture-summary.md#legacy-and-optional-surfaces).

The original "Frontend Development" section moved to
[Frontend Development](../ui/frontend-development-overview.md).

## Evolution since the original record

The original record listed three future items. Two are delivered at
`3b23d36f`:

- A custom web frontend replaced the Grafana dashboards: `packages/web-ui`,
  served by the server.
- Agent deployment: the agent applies the `desired_target` returned by the
  server (`packages/default/crates/cf-agent/src/deployment/`).

One remains open: support for CVE scanning tools beyond vulnix. This revision
documents only vulnix.

## Related concepts

- [Core components](../components/core-components.md) - component descriptions
- [Data flows](../architecture/data-flows.md) - data flow descriptions
- [Wakeups and polling](../architecture/event-driven-queues.md) - queue discovery and ordering
- [Observability and troubleshooting](../operations/observability-and-troubleshooting.md) - observability points
- [System overview](../overview/system-overview.md) - product-level orientation

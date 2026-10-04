---
type: Decision
title: "ADR-000: Crystal Forge Architecture Overview"
description: "Records the accepted architecture decision for Crystal Forge (status, context, key decisions, consequences, future evolution); open it for the rationale behind the shared database, Ed25519, Rust, event-driven queues, and flake-native design."
tags:
  - crystal-forge
  - decision
  - adr
  - architecture
implementation_status: partial
generated:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-03T22:57:15-05:00
sources:
  - id: origin
    resource: "Crystal Forge repository file docs/architecture.md at commit 3b23d36f"
    title: "ADR-000: Crystal Forge Architecture Overview"
---

# ADR-000: Crystal Forge Architecture Overview

## Status

Accepted

## Context

Crystal Forge provides compliance monitoring and build coordination for NixOS systems in regulated environments. The architecture must support horizontal scaling, cryptographic verification, and integration with existing compliance workflows.

## Decision

The detailed decision text was split into concepts: [Core components](../components/core-components.md), [Data flows](../architecture/data-flows.md), [Event-driven queue architecture](../architecture/event-driven-queues.md), and [Observability points](../operations/observability-and-troubleshooting.md). The key decisions follow.

### Key Architectural Decisions

1. **Shared PostgreSQL**: Enables horizontal scaling of servers and builders
2. **Ed25519 signatures**: Cryptographic verification of all agent communications
3. **Rust implementation**: Memory safety and performance for security-critical deployment
4. **Event-driven queues**: Immediate processing with coalesced wakeups and fallback polling
5. **Flake-native**: Direct integration with modern Nix ecosystem
6. **Isolated snapshot exploration**: PRIMARY emits the revision-scoped flake
   projection but does not inspect per-host option trees or module graphs. The
   durable Config Inspector worker runs separately, and inspection failure does
   not block builds or deployments.
7. **Option metadata authority**: Packaged NixOS option metadata is an authoring baseline, while each target flake's evaluation remains authoritative. See [NixOS Option Metadata Authority](../evaluation/nixos-option-metadata-authority.md).

## Consequences

**Positive**:

- Horizontal scaling through shared database
- Strong cryptographic security model
- Integration with existing monitoring infrastructure (Grafana)
- Memory-safe implementation reduces attack surface

**Negative**:

- PostgreSQL becomes single point of failure (mitigated by standard HA practices)
- Rust learning curve for contributors
- Initial dependency on Grafana for user interface

> **Status:** The "Frontend Development" section of the source document moved to [Frontend Development](../ui/frontend-development-overview.md).

## Future Evolution

- Custom web frontend to replace Grafana dashboards
- Agent deployment capabilities for configuration management
- Support for additional CVE scanning tools beyond vulnix

> **Status:** partial. A custom web frontend exists (`packages/web-ui`) and the agent deploys configurations (`packages/default/crates/cf-agent/src/deployment/`), so the first two future items look delivered. Additional CVE scanning tools beyond vulnix were not checked. The "Initial dependency on Grafana for user interface" consequence is a verification candidate.

## Related concepts

- [Core components](../components/core-components.md) - component descriptions
- [Data flows](../architecture/data-flows.md) - data flow descriptions
- [Event-driven queue architecture](../architecture/event-driven-queues.md) - queue notification design
- [Observability and troubleshooting](../operations/observability-and-troubleshooting.md) - observability points
- [System overview](../overview/system-overview.md) - product-level orientation

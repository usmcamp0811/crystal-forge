---
type: Concept
title: "Crystal Forge system context"
description: "Describes the system context, upstream dependencies, components, communication patterns, and scaling model of Crystal Forge; open it for orientation on what the platform depends on and how its parts connect."
tags:
  - crystal-forge
  - overview
  - context
implementation_status: implemented
generated:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-04T16:20:00-05:00
sources:
  - id: origin
    resource: "Crystal Forge repository file docs/context.md at commit 3b23d36f"
    title: "Crystal Forge Context & Current State"
  - id: code-1
    resource: "Crystal Forge repository file packages/default/crates/cf-server/src/bin/server.rs at commit 3b23d36f"
    title: Server routes
  - id: code-2
    resource: "Crystal Forge repository file packages/default/crates/cf-agent/src/deployment/agent.rs at commit 3b23d36f"
    title: Agent target handling
---

# Crystal Forge system context

Crystal Forge is a distributed monitoring, build coordination, and compliance
tooling system for NixOS fleets. Its goal is auditability and control of fleet
configuration, including in regulated environments. The server crate is
version 0.3.0.

## Upstream dependencies

- **Nix ecosystem:** the Nix evaluator, store, and flake system.
- **NixOS systems:** the managed hosts whose configurations Crystal Forge tracks.
- **PostgreSQL:** the system of record. Only the server connects to it.
- **Binary caches:** Nix, S3, or Attic destinations that builders publish to and
  agents pull from.

## System components

- **Server:** authoritative evaluation, policy, persistence, authentication and
  authorization, job coordination, and the embedded web UI. See
  [Core components](../components/core-components.md).
- **Builder:** API-only worker that realizes derivations and publishes them.
  It has no database access.
- **Agent:** runs on each managed NixOS system, sends signed reports, and
  applies the desired target that the server returns.
- **Web UI:** the Dioxus interface loaded from the server.

## Communication patterns

- Agent to server: HTTP POST with Ed25519-signed payloads to `/agent/heartbeat`
  and `/agent/state`. The server's response can carry the `desired_target`. The
  agent starts every exchange. The server does not initiate a connection to an
  agent.
- Builder to server: signed, session-checked requests to `/api/v1/builders/...`.
  The builder polls for jobs and sends heartbeats. The server does not push work
  to builders.
- Web UI to server: HTTP API requests authorized by the user's session and role.

## Scaling model

Build capacity scales by registering additional API builders and assigning them
to environments. Builders hold no database connection, so adding builders does
not add database clients. This page makes no claim about running more than one
server process against one database.

## Related concepts

- [System overview](system-overview.md) - what the platform is and how it is composed
- [Ecosystem architecture summary](../architecture/ecosystem-architecture-summary.md) - the one-page diagram
- [Core components](../components/core-components.md) - the agent, server, builder, and web UI in detail
- [Roadmap](roadmap.md) - planned work

---
type: Concept
title: "Crystal Forge system overview"
description: "Explains what Crystal Forge is, the fleet-management problems it solves, which component owns each responsibility, and where to read next; open it first for orientation before the detailed concepts."
tags:
  - crystal-forge
  - overview
  - architecture
  - orientation
implementation_status: implemented
generated:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-04T16:15:00-05:00
sources:
  - id: origin
    resource: "Crystal Forge repository file docs/specs/00-system-overview.md at commit 3b23d36f"
    title: "Crystal Forge - System Overview"
  - id: code-1
    resource: "Crystal Forge repository file packages/default/crates/cf-server/src/bin/server.rs at commit 3b23d36f"
    title: Server routes
  - id: code-2
    resource: "Crystal Forge repository file packages/default/crates/cf-server/src/handlers/ui.rs at commit 3b23d36f"
    title: Embedded UI serving
---

# Crystal Forge - System Overview

## What is Crystal Forge?

Crystal Forge is a **NixOS fleet management platform**: a control center for
tracking, building, deploying, and checking many NixOS systems.

### The core problem it solves

When you run multiple NixOS machines (servers, workstations, VMs), you need to:

1. Track what flake revision each machine runs.
2. Deploy configuration changes to machines.
3. Monitor which machines are healthy and whether they drift from the intended
   configuration.
4. Build Nix derivations on dedicated builders.
5. Cache built derivations for faster deployments.
6. Show vulnerability and compliance evidence for the fleet.

Crystal Forge provides a **Dioxus web UI** and an **HTTP API** to do all of
this from one place. The server serves the UI and authorizes every request.

## Architecture in one paragraph

The server is the single authority. It evaluates flake commits, applies policy,
owns PostgreSQL, authenticates users, and coordinates jobs. Builders are
API-only: they poll the server, build with Nix, and publish to a binary cache
without any database access. Agents run on managed hosts, send signed reports,
and receive the desired target in the server's response. The web UI is loaded
from the server and uses only the server API. The diagram, with the meaning of
every arrow, is in the
[Ecosystem architecture summary](../architecture/ecosystem-architecture-summary.md).

## Key components

1. **Server** (Axum) - REST API, sessions and RBAC, authoritative evaluation,
   job coordination, CVE and compliance workflows, the embedded UI.
2. **Web UI** (Dioxus) - dashboards and management views that call the API.
3. **Database** (PostgreSQL) - all persistent data, accessed only by the server.
4. **Builders** - API-only worker processes that build and publish.
5. **Agents** - processes on NixOS systems that report state and apply the
   desired target.

## How it works

The walkthroughs for registering a system, the evaluation and build queue
pipeline, deploying to a system, authentication, and authorization are in
their own concepts: [Authentication, authorization, and system registration](../security/authentication-and-authorization-overview.md),
[Evaluation and build queue pipeline](../workflows/evaluation-and-build-queue-pipeline.md),
and [Deployment flow](../deployment/deployment-flow.md).

## Where to read next

- HTTP API: [API overview](../api/api-overview-errors-and-streaming.md) and the
  [API index](../api/index.md).
- UI views: [Web UI navigation and shared patterns](../ui/frontend-navigation-and-shared-patterns.md)
  and the [UI index](../ui/index.md).
- Architecture decisions: [ADR-000](../decisions/adr-000-architecture-overview.md).

## Related concepts

- [Ecosystem architecture summary](../architecture/ecosystem-architecture-summary.md) - the current one-page diagram
- [Core components](../components/core-components.md) - what each component owns
- [Core entities and relationships](../data-model/core-entities-and-relationships.md) - the data model summarized here
- [Evaluation and build queue pipeline](../workflows/evaluation-and-build-queue-pipeline.md) - how commits flow through evaluation and builds
- [Deployment flow](../deployment/deployment-flow.md) - how systems receive a configuration
- [Authentication, authorization, and system registration](../security/authentication-and-authorization-overview.md) - login modes, roles, and registering a system
- [Local development workflow](../operations/local-development-workflow.md) - running the stack and key files
- [Server configuration reference](../operations/server-configuration-reference.md) - TOML and environment configuration

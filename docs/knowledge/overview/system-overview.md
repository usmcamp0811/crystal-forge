---
type: Concept
title: "Crystal Forge system overview"
description: "Explains what Crystal Forge is, the core fleet-management problem it solves, its high-level architecture, and its key components; open it first for orientation before the detailed concepts."
tags:
  - crystal-forge
  - overview
  - architecture
  - orientation
implementation_status: implemented
generated:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-03T22:57:15-05:00
sources:
  - id: origin
    resource: "Crystal Forge repository file docs/specs/00-system-overview.md at commit 3b23d36f"
    title: "Crystal Forge - System Overview"
---

# Crystal Forge - System Overview

> **Status:** The remaining sections of this source document were split into concepts. See Related concepts for their new locations. The diagram below places Builders beneath PostgreSQL. This is a verification candidate: the repository boundary is that API-only builders do not access the Crystal Forge database directly (`packages/default/crates/cf-builder/src/builder/api_client.rs`). The text below mentions `../architecture.md`, which was split into [ADR-000](../decisions/adr-000-architecture-overview.md) and the architecture concepts.

## What is Crystal Forge?

Crystal Forge is a **NixOS fleet management platform** - think of it like a control center for managing multiple NixOS systems at scale.

### The Core Problem It Solves

When you have multiple NixOS machines (servers, workstations, VMs), you need to:
1. Track what flake/revision each machine is running
2. Deploy configuration changes to machines
3. Monitor which machines are healthy
4. Build Nix derivations in parallel
5. Cache built derivations for faster deployments

Crystal Forge provides a **web UI** and **API** to do all of this from one place.

## High-Level Architecture

```
┌─────────────────────────────────────────────────────────┐
│                    Web UI (Dioxus)                      │
│  - Dashboard, Systems, Flakes, Builds, Admin           │
└─────────────────────┬───────────────────────────────────┘
                      │ HTTP API
┌─────────────────────▼───────────────────────────────────┐
│                  Axum API Server                       │
│  - REST endpoints                                      │
│  - Session management                                 │
│  - Authorization (RBAC)                               │
└──────────┬──────────────────┬───────────────────────────┘
           │                  │
    ┌──────▼──────┐    ┌──────▼──────┐
    │  PostgreSQL │    │    Git      │
    │  Database   │    │  (Flakes)   │
    └─────────────┘    └─────────────┘
           │
    ┌──────▼──────┐    ┌──────▼──────┐
    │  Builders   │    │   Agents    │
    │  (Workers)  │    │  (Systems)  │
    └─────────────┘    └─────────────┘
```

### Key Components

1. **Web UI** - Dioxus frontend (React-like, but Rust)
2. **API Server** - Axum HTTP server with REST API
3. **Database** - PostgreSQL for all persistent data
4. **Builders** - Worker processes that build Nix derivations
5. **Agents** - NixOS systems that report to the server

## How It Works

The walkthroughs for registering a system, the evaluation and build queue pipeline, deploying to a system, authentication, and authorization moved to their own concepts. See [Authentication, authorization, and system registration](../security/authentication-and-authorization-overview.md), [Evaluation and build queue pipeline](../workflows/evaluation-and-build-queue-pipeline.md), and [Deployment flow](../deployment/deployment-flow.md).

## Next Steps

For detailed API endpoints, see `02-backend-api.md`
For detailed UI views, see `01-frontend-views.md`
For architecture decisions, see `../architecture.md`

## Related concepts

- [Core components](../components/core-components.md) - agent, server, and builder responsibilities
- [Core entities and relationships](../data-model/core-entities-and-relationships.md) - the data model summarized here
- [Evaluation and build queue pipeline](../workflows/evaluation-and-build-queue-pipeline.md) - how commits flow through evaluation and builds
- [Deployment flow](../deployment/deployment-flow.md) - how systems receive a configuration
- [Authentication, authorization, and system registration](../security/authentication-and-authorization-overview.md) - login modes, roles, and registering a system
- [Local development workflow](../operations/local-development-workflow.md) - running the stack and key files
- [Server configuration reference](../operations/server-configuration-reference.md) - TOML and environment configuration
- [ADR-000 Architecture overview](../decisions/adr-000-architecture-overview.md) - the recorded architecture decision

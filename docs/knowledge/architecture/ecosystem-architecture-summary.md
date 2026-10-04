---
type: Architecture
title: Ecosystem architecture summary
description: Shows the high-level flowchart of agent, server, builder, binary cache, PostgreSQL, Grafana, and web UI, and a four-row component table; open it for the one-page picture before the detailed architecture concepts.
tags:
  - crystal-forge
  - architecture
  - components
  - overview
implementation_status: partial
generated:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-04T08:25:07-05:00
sources:
  - id: origin
    resource: "Crystal Forge repository file README.md at commit 3b23d36f"
    title: Crystal Forge README
---

# Ecosystem architecture summary

> **Status:** partial. This concept holds the `Architecture` and `Components` sections of the repository `README.md`. The diagram shows the agent connecting to the server with Ed25519 signatures. It also shows an arrow from the server to the builder and from the builder to PostgreSQL. The repository architecture rules in `AGENTS.md` state that API-only builders do not access the database directly (see [Builder architecture and job scheduling](../builders/builder-architecture-and-job-scheduling.md)). The arrows `S --> B` and `B <--> P` are therefore verification candidates against `packages/default/crates/cf-builder/` and `packages/default/crates/cf-server/`.

## Architecture

```mermaid
flowchart LR
    C["Binary Cache<br/>S3/Attic/Nix"]
    A["Agent<br/>NixOS hosts"]

    subgraph "Core Infrastructure"
        S["Server<br/>API/UI"]
        B["Builder<br/>Evaluation/CVE"]
        P["PostgreSQL<br/>State"]
        G["Grafana<br/>(optional)"]
    end

    subgraph "Web UI (Dioxus)"
        UI["Dashboard/Systems/Flakes/Builds/CVEs"]
        API["API Client"]
    end

    A -->|Ed25519 signed| S
    S --> B
    B -->|Push| C
    C -->|Pull| A
    B <--> P
    P --> G
    UI --> API
    API --> S

    classDef default fill:#f9f9f9,stroke:#333,stroke-width:2px,color:#000
    classDef external fill:#e8e8e8,stroke:#666,stroke-width:2px,color:#000
    class C external
```

## Components

| Component      | Description                                                            |
| -------------- | ---------------------------------------------------------------------- |
| **Agent**      | Runs on each NixOS host, monitors config changes, reports fingerprints |
| **Server**     | API, web UI, coordinates builds, manages deployments                   |
| **Builder**    | Evaluates NixOS flakes, builds derivations, runs CVE scans             |
| **PostgreSQL** | Centralized state, user/role data, compliance history                  |

## Related concepts

- [Core components](../components/core-components.md)
- [Data flows](data-flows.md)
- [Crystal Forge system overview](../overview/system-overview.md)
- [Project introduction and key features](../overview/project-introduction-and-key-features.md)

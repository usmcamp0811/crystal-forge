---
type: Data Model
title: "Core entities and relationships"
description: "Summarizes the core Crystal Forge entities (system, environment, flake, builder, user, deployment, derivation, cache) and their relationships; open it for a quick map of the domain model."
tags:
  - crystal-forge
  - data-model
  - entities
  - relationships
implementation_status: implemented
generated:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-03T22:57:15-05:00
sources:
  - id: origin
    resource: "Crystal Forge repository file docs/specs/00-system-overview.md at commit 3b23d36f"
    title: "Crystal Forge - System Overview"
---

# Data Model

## Core Entities

| Entity | Description |
|--------|-------------|
| **System** | A NixOS machine (physical or virtual) that reports to CF |
| **Environment** | Grouping for systems (prod, staging, dev) |
| **Flake** | A Nix flake registry entry (git repo + branch) |
| **Builder** | Worker that builds derivations |
| **User** | Admin/Operator/Viewer accounts |
| **Deployment** | Record of a system being deployed |
| **Derivation** | A Nix derivation being built |
| **Cache** | Binary cache for built derivations |

## Relationships

```mermaid
%% diagram-id: core-entity-relationships-conceptual
%% These are source-described conceptual relationships, not migration-derived
%% ER cardinalities or claims about current schema constraints.
erDiagram
    ENVIRONMENT ||--o{ SYSTEM : "source-described grouping"
    ENVIRONMENT ||--o{ USER : "via membership"
    FLAKE ||--o{ DEPLOYMENT : "source-described"
    FLAKE ||--o{ COMMIT : "git history"
    BUILDER }o--o{ DERIVATION : "builds"
    BUILDER }o--o{ ENVIRONMENT : "serves"
    SYSTEM }o--o{ DEPLOYMENT : "has history of"
```

> The diagram encodes the source document's conceptual relationship sketch.
> Its cardinality notation is not derived from migrations and MUST NOT be read
> as a statement of current database constraints.

## Related concepts

- [System overview](../overview/system-overview.md) - product orientation
- [Derivation status lifecycle](../concepts/derivation-status-lifecycle.md) - states of the derivation entity

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

```
Environment 1──∞ System
    │
    └──∞ Users (via membership)

Flake 1──∞ Deployment
    │
    └──∞ Commit (git history)

Builder ∞──∞ Derivation (builds)
Builder ∞──∞ Environment (serves)

System ∞──∞ Deployment (has history of)
```

## Related concepts

- [System overview](../overview/system-overview.md) - product orientation
- [Derivation status lifecycle](../concepts/derivation-status-lifecycle.md) - states of the derivation entity

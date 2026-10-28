---
type: Decision
title: "Config Explorer decision record"
description: "Records the ten accepted Config Explorer decisions (observational only, lazy scoped browsing, optional V2 snapshots, exact-identity caching, no client Nix expressions) and the rule that violating work must amend the architecture."
tags:
  - crystal-forge
  - config-explorer
  - decision
  - task-440
implementation_status: implemented
generated:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-03T22:57:42-05:00
sources:
  - id: origin
    resource: "Crystal Forge repository file docs/config-explorer-architecture.md at commit 3b23d36f"
    title: "Config Explorer Architecture"
---

# Config Explorer decision record

## Decision record

- **Decision 1:** Config Explorer is observational, not deployment authority.
- **Decision 2:** Policy enforcement continues to evaluate policies independently.
- **Decision 3:** Config browsing is lazy and path-scoped.
- **Decision 4:** Complete V2 snapshots are optional enrichment and comparison artifacts.
- **Decision 5:** Partial or unreadable branches remain local failures.
- **Decision 6:** Waiting for evaluator capacity is distinct from running inspection.
- **Decision 7:** Explorer results are cached against immutable exact target identity.
- **Decision 8:** Arbitrary Nix expressions are never accepted from clients.
- **Decision 9:** Exact revision inspection is read-only.
- **Decision 10:** Cached or partial Explorer data cannot produce authoritative Changed or Drift conclusions.

Future work that violates one of these decisions MUST amend this document as
an explicit architecture change instead of silently changing the behavior.

## Related concepts

* [Config Explorer Architecture](../evaluation/config-explorer-architecture.md) - Specifies the Config Explorer design: why full option crawls are the wrong prerequisite, the three-evaluator authority invariants, the phased lazy inspection model, the Configured options classifier, and the benchmark record.
* [Config Explorer current implementation map](../evaluation/config-explorer-implementation-status.md) - Maps the Config Explorer design to its implementing server, worker, Nix expression, query, migration, API, and Web UI paths, and describes how scoped observations, V2 snapshot reuse, and paged root and prefix observations currently work.

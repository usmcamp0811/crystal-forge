---
type: Design Specification
title: "Config Explorer target identity, cache contract, and snapshot semantics"
description: "Specifies upgraded-fleet current revision recovery, the immutable target identity and cache contract, the split between Explorer observations and certified V2 snapshots, and the Changed, Drift, and search semantics."
tags:
  - crystal-forge
  - config-explorer
  - evaluation
  - cache
  - identity
implementation_status: implemented
generated:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-03T22:57:42-05:00
sources:
  - id: origin
    resource: "Crystal Forge repository file docs/config-explorer-architecture.md at commit 3b23d36f"
    title: "Config Explorer Architecture"
---

# Config Explorer target identity, cache contract, and snapshot semantics

## Target identity and cache contract

### Upgraded-fleet current revision recovery

The server can recover an observational current commit when an upgraded agent
reports only its current generation and store path. One shared read-only
resolver applies this priority:

1. Exact retained generation identity with verified store lineage.
2. One server-issued successful deployment identity that binds the same
   system, observed store path, requested commit and derivation, system flake,
   and effective configuration. The deployment event must precede or coincide
   with the observation.
3. One legacy NixOS derivation whose exact effective configuration and
   `COALESCE(store_path, expected_store_path)` match the nonempty observed store
   path on the system's current flake.

An explicit `generation_matches_current_store_path = false`, an empty current
store path, a foreign flake, a different configuration, more than one distinct
candidate commit, or a non-full commit SHA produces no mapping. The resolver
MUST NOT infer identity from repository head, commit recency, host name alone,
store-path basenames, or a foreign flake.

The recovered identity is only a label for read-only Config navigation. It
MUST NOT grant rollback eligibility, retained lineage, deployment authority,
policy authority, or permission to start Config work. Those decisions retain
their independent authorization and exact-evidence checks.

`SystemCommitsResponse.current_commit` is the sole current-mode observational
commit authority. The UI MUST NOT replace an absent value with a generation
commit, repository head, or newest timeline commit. An explicit historical
generation can continue to use its own full commit identity. Retained rollback
evidence remains authoritative when its stored commit identifier is invalid,
but the invalid identifier MUST NOT enter Config navigation.

Commit mode can start a targeted Config observation only for a full commit SHA
whose exact NixOS derivation has an exact configuration name, completion time,
and nonempty derivation path. The whole commit can remain `in_progress`: one
configuration becomes inspectable as soon as its derivation and root are
persisted. The UI selects the newest commit that satisfies these per-system
prerequisites. It does not assume that the first timeline row is inspectable.
Only an authenticated Admin can start root, scoped, or configured-index
observations. An Operator retains ordinary system mutation permissions but
cannot start Config observations. An explicit commit selection remains
historical observation context even if its SHA equals the recovered current
label.

Every observation is tied to an immutable identity containing, at minimum:

- commit or revision;
- configuration name;
- exact derivation or carrier;
- path components;
- observation type; and
- inspection schema version.

Results from one carrier or revision MUST NOT be reused for another carrier or
revision. A cache key MUST include every field that can change the inspected
configuration or interpretation of its result.

The cache contract is:

- A cache hit starts zero Nix subprocesses.
- Identical active requests coalesce.
- Different targets run independently.
- An uncached shallow request evaluates the exact materialized immutable source
  for its commit. It does not reconstruct the source from a mutable branch or
  evaluate `system.build.toplevel`.
- A complete V2 snapshot MAY satisfy Explorer reads immediately when its
  authority and completeness contract match the request.

## Explorer observations and certified V2 snapshots

Explorer observations and certified V2 Config snapshots serve different
purposes.

### Explorer observations

Explorer observations are scoped, lazy, potentially incomplete, suitable for
browsing, and non-authoritative. They describe what a bounded request
observed. They do not prove that an uninspected path is absent.

### Certified V2 Config snapshot

A certified V2 snapshot is a coherent inventory artifact. It is complete or
explicitly partial, and it is required for authoritative comparison semantics.
It MAY be generated deliberately or as background enrichment. It remains useful
for Changed, Drift, complete search, and full-corpus analysis.

Explorer observations MUST NOT automatically become a certified snapshot. The
partial-inventory contract preserves these fields and their meanings:

- `option_inventory_complete`;
- `diagnostics`;
- `diagnostics_truncated`; and
- `comparison_ready`.

## Changed, Drift, and search semantics

Changed and Drift require a sufficiently complete certified inventory. Lazy or
partial Explorer data MUST NEVER imply absence:

```text
incomplete data != zero changes
incomplete data != no drift
```

Search over a complete V2 snapshot MAY provide complete search semantics. Search
over only a lazy Explorer cache MUST identify that it is limited to inspected
and cached paths, or report that complete search is unavailable. It MUST NOT
silently present a partial search as a complete corpus search.

The Web UI uses certified server search only when `OptionInventoryState` is
`Complete`. For a partial or unavailable inventory, search filters only the
structured option identities, values, and provenance already observed in the
current Explorer session. It does not start prefix, option, or provenance
requests to expand the result set.

Scoped observations use an exact commit identity. A retained-generation
selection therefore shows Browse and Configured as locally unavailable. This
restriction does not disable certified Search or Sources data for that retained
generation.

## Related concepts

* [Config Explorer Architecture](config-explorer-architecture.md) - Specifies the Config Explorer design: why full option crawls are the wrong prerequisite, the three-evaluator authority invariants, the phased lazy inspection model, the Configured options classifier, and the benchmark record.
* [Evaluation snapshot identity, comparison, and lifecycle](evaluation-snapshot-identity-lifecycle-and-comparison.md) - Specifies full-SHA snapshot identity, first-parent resolution and Changed comparison rules, and the queued/running/failed/available/unavailable lifecycle for Config snapshot reads and the targeted Config inspection mutation.
* [Config Explorer failure containment, scheduling, security, and API principles](config-explorer-resource-security-and-api-model.md) - Covers Config Explorer failure containment, resource scheduling priority and capacity states, request lifecycle, security rules, process and timeout model, optional complete inventory, API principles, data flow, non-goals, and future evolution.

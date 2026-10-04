---
type: Design Specification
title: "Config Explorer Architecture"
description: "Specifies the Config Explorer design: why full option crawls are the wrong prerequisite, the three-evaluator authority invariants, the phased lazy inspection model, the Configured options classifier, and the benchmark record."
tags:
  - crystal-forge
  - config-explorer
  - evaluation
  - architecture
  - invariants
implementation_status: implemented
generated:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-03T22:57:42-05:00
sources:
  - id: origin
    resource: "Crystal Forge repository file docs/config-explorer-architecture.md at commit 3b23d36f"
    title: "Config Explorer Architecture"
---

# Config Explorer Architecture

**Status:** Accepted / implemented initially by TASK-440

**Scope:** Crystal Forge Config inspection and exploration

**Related:** TASK-440, MR !323

> **Status:** accepted and implemented initially by TASK-440. The current code map is in [config-explorer-implementation-status.md](config-explorer-implementation-status.md). The decisions are in [config-explorer-decisions.md](../decisions/config-explorer-decisions.md).

## Problem statement

Crystal Forge configurations can expose more than 16,000 NixOS options. A
complete option, value, and provenance crawl is expensive in CPU, memory, and
wall-clock time. Deployed evidence included a Stage-1 inspection that reached
the 300-second timeout and a Config job that remained marked `running` while it
waited about 1,432 seconds for the global heavy-Nix advisory lock. An
all-at-once crawl also allows one poisoned branch to reduce the usability of
the whole view.

Users should not need a complete corpus to inspect one setting. Full
configuration extraction is therefore the wrong prerequisite for interactive
browsing.

## Architecture invariants

Crystal Forge keeps three distinct evaluators. Their outputs have different
authority and failure semantics.

### Primary evaluator

The primary evaluator evaluates the NixOS configuration and determines the
authoritative build and deployment pipeline outputs. Its result controls the
evaluation/build/deployment pipeline.

### Policy evaluator

The policy evaluator evaluates deployment and compliance assertions directly
against the exact target configuration. For example, a policy may evaluate
`config.services.openssh.enable == true`. Its result is the authoritative
policy `PASS`, `FAIL`, or `ERROR` result used by deployment gating.

### Config Explorer

The Config Explorer helps a human inspect and understand a configuration. Its
observations are non-authoritative. Config Explorer data MUST NEVER become the
source of truth for deployment policy enforcement.

The following invariants are mandatory:

- Policy evaluation MUST work when no Config Explorer cache exists.
- Policy evaluation MUST work when Explorer data is partial.
- A poisoned Explorer branch MUST NOT change policy semantics.
- Opening Config MUST NOT be required before deployment.
- An Explorer cache hit MUST NOT substitute for an authoritative policy Nix
  evaluation.
- Primary and policy evaluation MUST remain independent of Explorer cache
  completeness, freshness, and availability.

```text
                    Exact NixOS configuration
                             |
             +---------------+---------------+
             |                               |
             v                               v
      Policy evaluator                 Config Explorer
             |                               |
       PASS/FAIL/ERROR                  observational only
             |                               |
      deployment gate             lazy/cacheable inspection
```

## High-level Config path

Config inspection follows the exact target through increasingly narrow work:

```text
exact commit/config/carrier
          |
          v
shallow root inspection
          |
          v
user expands prefix
          |
          v
scoped prefix inspection
          |
          v
user selects option
          |
          v
value/metadata inspection
          |
          v
optional provenance detail
```

The interface is REPL-like in interaction, but it is not an actual REPL. The
initial top-level tree should appear quickly. Nodes expand on demand. Option
details load only when selected. Expensive provenance loads only when
requested or required by the selected detail. Observations for immutable
targets can be cached and reused.

Crystal Forge MUST NOT expose `nix repl` to clients. A persistent client REPL
would create arbitrary-code risk, credential-lifecycle problems, state
contamination, process-lifetime and concurrency problems, and a fragile
protocol boundary. Clients send structured path, depth, and kind requests.
The server builds trusted Nix expressions from validated inputs.

## Inspection phases

The phased model is:

### Phase 0: exact target resolution

Resolve the exact commit or revision, configuration name, and derivation or
carrier. Reject ambiguous, unauthorized, or mismatched targets before Nix
work starts.

### Phase 1: shallow root/bootstrap

Establish a trustworthy top-level hierarchy and basic metadata. This phase MAY
inspect the root structure and bounded diagnostics. It SHOULD avoid evaluating
all option values and all provenance records.

The primary evaluator includes this bounded root payload with each successful
configuration result. The derivation transaction also creates a root-only
fallback for the exact active system. This fallback makes the handoff durable
before the evaluator continues. An asynchronous worker replaces the fallback
with a valid in-band payload, so payload persistence does not delay evaluator
output consumption or build dispatch. If the payload is absent, invalid,
cannot be published, or cannot enter the bounded publication channel, the
fallback remains queued. The worker also runs a per-commit catch-up after it
drains the channel.

### Phase 2: scoped prefix expansion

Inspect only the requested path prefix and bounded descendants. Expanding a
prefix MUST NOT require evaluating every option value or every provenance
record in the configuration.

### Phase 3: exact option detail

Inspect the selected option's declared type, safe value or error, and basic
metadata. The request remains scoped to the exact option.

### Phase 4: detailed provenance

Inspect definitions, source input, revision, path, and related provenance only
for the selected option or explicitly requested detail. Provenance work MAY be
more expensive than tree navigation and MUST retain the same target identity.

## Configured options

The Configured options index is a separate asynchronous observation. It is not
the root observation, and root rendering MUST NOT wait for it. Opening Config
starts only the shallow root observation. The first activation of the
Configured mode starts the index. The Explorer caches and reuses that result for
the exact target until the revision changes or the user explicitly retries a
failed request. This lazy trigger prevents an O(N) traversal for users who only
browse scoped paths.

An option appears in Configured options if and only if the exact evaluated
module configuration contains a surviving non-default configuration
definition. `option.isDefined` alone is not sufficient because a declaration
default also makes an option defined. The classifier derives `defaultPriority`
from `(lib.mkOptionDefault null).priority` and applies this exact rule:

```text
configured = option.isDefined && (
    !(option ? default)
    || highestPrio < defaultPriority
    || (highestPrio == defaultPriority && post-filter survivor count > 1)
)
```

The survivor count uses definitions after module priority filtering. Therefore
a priority-2000 assignment that loses to a declaration default is absent. An
ordinary assignment, `mkDefault`, `mkForce`, a surviving module-generated
assignment, a priority-1500 tie with a declaration default, and a defaultless
option assigned with `mkOptionDefault` are present.

The classifier MUST NOT read `option.value`, execute `apply`, encode a value,
load complete provenance, or read definition values. Nix 2.34 `tryEval` does
not contain every type error. Each option classifier is therefore an
independent `nix-eval-jobs` job. A failing ambiguous priority-1500 survivor
count becomes one bounded diagnostic. Healthy option identities remain in the
result.

The configured-index payload contains only exact `path_components`, option keys,
bounded diagnostics, and traversed/configured identity counts. One request
stores this bounded index as one content row. It retains at most 512 configured
identities and reports the untruncated total. Option detail, values, and
provenance remain separate lazy observations.

### Benchmark record

The pre-implementation configured/highest-priority baseline traversed 16,266
option identities and returned 226 configured identities. Across the observed
runs, wall time had a 7.31-second median and a 7.30-7.42-second range. Peak RSS
had a 667,692-KiB median and a 667,052-667,852-KiB range. The run used one
evaluator client and no child processes; the external Nix daemon was excluded.

The shallow-root baseline returned 54 entries. Wall time had a 4.97-second
median and a 4.92-5.06-second range. Peak RSS had a 204,504-KiB median and a
204,276-204,564-KiB range.

These measurements justify a lazy configured-index request: configured-index
latency and memory MUST NOT become Config-open or root latency and memory. The baseline did
not include the final exact tie classifier. The final implementation isolates
ambiguous classifier jobs as required above. Its focused real-Nix regression
proves failure localization and value non-evaluation, but this change does not
claim a comparable production-scale final-implementation benchmark. Record
that benchmark when the exact production fixture and measurement harness are
available.

## Related concepts

* [Config Explorer target identity, cache contract, and snapshot semantics](config-explorer-target-identity-and-snapshot-semantics.md) - Specifies upgraded-fleet current revision recovery, the immutable target identity and cache contract, the split between Explorer observations and certified V2 snapshots, and the Changed, Drift, and search semantics.
* [Config Explorer failure containment, scheduling, security, and API principles](config-explorer-resource-security-and-api-model.md) - Covers Config Explorer failure containment, resource scheduling priority and capacity states, request lifecycle, security rules, process and timeout model, optional complete inventory, API principles, data flow, non-goals, and future evolution.
* [Config Explorer current implementation map](config-explorer-implementation-status.md) - Maps the Config Explorer design to its implementing server, worker, Nix expression, query, migration, API, and Web UI paths, and describes how scoped observations, V2 snapshot reuse, and paged root and prefix observations currently work.
* [Config Explorer decision record](../decisions/config-explorer-decisions.md) - Records the ten accepted Config Explorer decisions (observational only, lazy scoped browsing, optional V2 snapshots, exact-identity caching, no client Nix expressions) and the rule that violating work must amend the architecture.
* [Evaluation and Flake Snapshot Architecture](evaluation-flake-snapshot-architecture.md) - Defines ownership and data flow for revision-specific evaluation and flake-output snapshots: what PRIMARY and the Config Inspector worker own, immutable artifacts versus the mutable selector, deployment binding, and database-only reads.

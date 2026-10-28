---
type: Architecture
title: "Data Flows"
description: "Describes the four core data flows - agent state reporting, commit evaluation and build, CVE scanning, and drift detection - between agent, server, builder, PostgreSQL, cache, and the UI, with who initiates each step; open it to trace how data moves through the system."
tags:
  - crystal-forge
  - architecture
  - data-flow
  - drift
  - cve
implementation_status: implemented
generated:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-04T16:05:00-05:00
sources:
  - id: origin
    resource: "Crystal Forge repository file docs/architecture.md at commit 3b23d36f"
    title: "ADR-000: Crystal Forge Architecture Overview (original Data Flows section)"
  - id: code-1
    resource: "Crystal Forge repository file packages/default/crates/cf-server/src/handlers/agent_request.rs at commit 3b23d36f"
    title: Agent request signature verification
  - id: code-2
    resource: "Crystal Forge repository file packages/default/crates/cf-server/src/server/mod.rs at commit 3b23d36f"
    title: Server background tasks
  - id: code-3
    resource: "Crystal Forge repository file packages/default/crates/cf-server/src/queries/evaluation_snapshots.rs at commit 3b23d36f"
    title: Exact store-path drift and seven-day drift
  - id: code-4
    resource: "Crystal Forge repository file packages/default/crates/cf-server/src/builder/cve_worker.rs at commit 3b23d36f"
    title: Server CVE scan loop
---

# Data Flows

Every flow below passes through the server. The UI reads results through the
server API. Arrows labeled `request` start at the requester.

## 1. Agent state reporting

```mermaid
sequenceDiagram
    autonumber
    participant Host as Managed NixOS host
    participant Agent as cf-agent
    participant API as Server API
    participant DB as PostgreSQL
    participant UI as Dioxus UI

    Host->>Agent: configuration change or heartbeat timer
    Agent->>API: request: signed heartbeat or state report
    API->>API: verify signature against the system's public key
    API->>DB: persist state or heartbeat
    API-->>Agent: response (may carry desired_target)
    UI->>API: request: read systems and status
    API->>DB: read
    API-->>UI: response
```

`POST /agent/state` and `POST /agent/heartbeat` verify the `X-Signature`
header against the registered public key before the server persists anything.
Which of the two the server stores is decided by the equivalence check in
[Agent heartbeat versus state persistence](../deployment/agent-heartbeat-vs-state-persistence.md).

## 2. Commit evaluation, build, and cache

```mermaid
flowchart LR
    Source["Git remote<br/>(poll, webhook, API sync)"] -->|"commit inserted"| Server
    subgraph Server["Server (authoritative)"]
        Eval["Evaluate with nix-eval-jobs"]
        Jobs["Queue build jobs<br/>for derivations that pass admission"]
        DB[("PostgreSQL")]
        Eval --> Jobs
        Eval --> DB
        Jobs --> DB
    end
    Builder["API-only builder"] -->|"request: poll and claim"| Server
    Server -->|"response: job"| Builder
    Builder -->|"data transfer: outputs"| Cache[("Binary cache")]
    Builder -->|"request: complete with cache reference"| Server
    Server -.->|"probe"| Cache
```

The server evaluates; the builder realizes. See
[Wakeups and polling](event-driven-queues.md) for how each step discovers work.

## 3. CVE scanning

```mermaid
flowchart LR
    Built["Build completes and publishes"] --> Sched["Server scan scheduling<br/>(scan schedule policy)"]
    Sched --> Where{"Who runs the scan?"}
    Where -->|"server-local vulnix executor<br/>when enabled and vulnix is available"| Local["Server scan loop"]
    Where -->|"builder claims a leased scan<br/>through the API"| Remote["Builder-side scan"]
    Local --> Store["Scan results and exact evidence"]
    Remote -->|"request: submit results"| Store
    Store --> DB[("PostgreSQL")]
    DB --> UI["Dioxus UI: CVE inventory, triage, POA&M"]
```

The server's scan loop always recovers expired remote scan leases and
reconciles scan prerequisites. It runs vulnix itself only when the local
executor is enabled and vulnix is available. See
[Exact-CVE evidence authority](../cves/exact-cve-evidence-authority-and-inventory-reads.md).

## 4. Drift detection

```mermaid
flowchart LR
    Reported["Agent-reported running store path<br/>and heartbeat history"] --> Compare["Server comparison"]
    Selected["Server-evaluated expected store path<br/>of the selected configuration"] --> Compare
    Compare --> Exact["Exact store-path drift"]
    Compare --> Seven["Seven-day drift status"]
    Exact --> UI["Dioxus UI status"]
    Seven --> UI
```

The server compares the store path the agent reports as running with the store
path of the selected, server-evaluated configuration (`EvaluationDrift`). It
derives a separate seven-day drift status from persisted system-state and
heartbeat observations. The expected path comes from server evaluation, not
from a builder.

## 5. Evaluation and flake snapshot flow

PRIMARY evaluates system derivations and policies. It also emits one
revision-scoped flake-output projection without per-host exploration. A separate
durable Config Inspector worker can extract complete option metadata, safe
values, and module provenance for an exact commit and configuration after
PRIMARY succeeds. Config Explorer first reuses exact certified V2 or scoped
observations. It queues bounded observational Nix only when persisted evidence
cannot answer the requested scope. Explorer data never becomes policy or
deployment input. See
[Evaluation and Flake Snapshot Architecture](../evaluation/evaluation-flake-snapshot-architecture.md)
for ownership, lifecycle, comparison, retention, redaction, authorization, and
compatibility contracts. See
[Config Explorer Architecture](../evaluation/config-explorer-architecture.md) for lazy
observation, failure containment, cache, and authority boundaries.

## Related concepts

- [Ecosystem architecture summary](ecosystem-architecture-summary.md) - the one-page diagram
- [Core components](../components/core-components.md) - what each component owns
- [Wakeups and polling](event-driven-queues.md) - queue discovery and ordering
- [Commit to deploy flow](../workflows/commit-eval-build-cache-deploy-flow.md) - the end-to-end flow chart

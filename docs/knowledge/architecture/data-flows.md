---
type: Architecture
title: "Data Flows"
description: "Describes the four core data flows (state monitoring, CVE scanning, drift detection, evaluation and flake snapshots) between agent, server, builder, PostgreSQL, and Grafana; open it to trace how data moves through the system."
tags:
  - crystal-forge
  - architecture
  - data-flow
  - drift
  - cve
implementation_status: implemented
generated:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-03T22:57:15-05:00
sources:
  - id: origin
    resource: "Crystal Forge repository file docs/architecture.md at commit 3b23d36f"
    title: "ADR-000: Crystal Forge Architecture Overview"
  - id: code-1
    resource: "Crystal Forge repository file packages/default/crates/cf-server/src/handlers/agent_request.rs at commit 3b23d36f"
    title: Agent request signature verification
  - id: code-2
    resource: "Crystal Forge repository file packages/default/crates/cf-server/src/server/mod.rs at commit 3b23d36f"
    title: Server background loops
  - id: code-3
    resource: "Crystal Forge repository file packages/default/crates/cf-server/src/queries/evaluation_snapshots.rs at commit 3b23d36f"
    title: Drift summary queries
---

# Data Flows

> **Status:** Split from [ADR-000](../decisions/adr-000-architecture-overview.md). Flows 1 to 3 were compared with the code at the level of component roles (see Migration verification notes). Flow 2 was corrected: evaluation runs in the server, not in the builder. Flow 4 was not compared in detail; its owning concepts are in `evaluation/`.

## 1. State Monitoring Flow

```
NixOS System → Agent → Server → PostgreSQL → Grafana
```

Agent detects configuration change → Signs state report → Server validates signature → Stores compliance data → Grafana displays/alerts

## 2. CVE Scanning Flow

```
Git Webhook → Server → Builder → vulnix → PostgreSQL → Grafana
```

Configuration update → Server evaluates the flake commit (`nix-eval-jobs`) and queues a build job → Builder builds the derivation → CVE scan runs (vulnix, on a builder or in the server's scan loop) → Stores vulnerability data → Compliance dashboard updates

## 3. Drift Detection Flow

```
Agent State + Builder Evaluation → Server Comparison → Compliance Alert
```

Current system state compared against latest evaluated configuration to detect unauthorized changes.

## 4. Evaluation and Flake Snapshot Flow

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

- [Core components](../components/core-components.md) - the components named in these flows
- [Event-driven queue architecture](event-driven-queues.md) - queue wakeups behind the evaluation flow
- [Commit to deploy flow](../workflows/commit-eval-build-cache-deploy-flow.md) - the end-to-end flow chart

## Migration verification notes

Scope: component roles and direction of flows 1 to 3. Not checked: the full flow 4 text, Grafana dashboard behavior, and alerting.

- Claim: Flow 1: the agent signs the state report and the server validates the signature before storing it.
  Finding: `POST /agent/state` and `POST /system_state` verify the `X-Signature` header against the system's public key, then persist the state.
  Evidence: `cf-server/src/handlers/agent_request.rs`; `cf-server/src/handlers/agent/state.rs`; `cf-server/src/bin/server.rs` routes.
  Case: implemented. Grafana is an optional NixOS module integration (`modules/nixos/crystal-forge/default.nix`); alerting not checked.
- Claim: Flow 2: the builder evaluates the flake and runs the CVE scan; the webhook triggers the server.
  Finding: The server evaluates commits (`run_commit_evaluation_loop`). Builders build derivations. CVE scans run in the server loop (`run_cve_scan_loop`) or on builders (`cve-scans/claim`). The webhook inserts a commit only; polling or the fallback tick starts evaluation.
  Evidence: `cf-server/src/server/mod.rs`; `cf-server/src/builder/cve_worker.rs`; `cf-server/src/handlers/webhook.rs`; `cf-builder/src/builder/cve_scanner.rs`.
  Case: documentation stale (corrected in place).
- Claim: Flow 3: current state is compared with the latest evaluated configuration to detect unauthorized changes.
  Finding: The server computes selected-versus-running store-path drift (`EvaluationDrift`) and a seven-day drift status from stored system-state and heartbeat observations. A "compliance alert" action on drift was not checked.
  Evidence: `cf-server/src/queries/evaluation_snapshots.rs`; `cf-server/src/handlers/api/systems.rs`.
  Case: partially verified; alerting not checked.

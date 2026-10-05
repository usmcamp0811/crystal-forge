---
type: Component
title: "Core Components"
description: "Describes what the Crystal Forge server, builder, agent, and web UI each own, their interfaces, and which connections each initiates; open it to see where authority and trust boundaries sit."
tags:
  - crystal-forge
  - components
  - agent
  - server
  - builder
  - web-ui
implementation_status: implemented
generated:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-04T15:55:00-05:00
sources:
  - id: origin
    resource: "Crystal Forge repository file docs/architecture.md at commit 3b23d36f"
    title: "ADR-000: Crystal Forge Architecture Overview (original Core Components section)"
  - id: code-1
    resource: "Crystal Forge repository file packages/default/crates/cf-server/src/bin/server.rs at commit 3b23d36f"
    title: Server routes
  - id: code-2
    resource: "Crystal Forge repository file packages/default/crates/cf-server/src/server/mod.rs at commit 3b23d36f"
    title: Server background tasks
  - id: code-3
    resource: "Crystal Forge repository file packages/default/crates/cf-builder/src/bin/builder.rs at commit 3b23d36f"
    title: Builder process
  - id: code-4
    resource: "Crystal Forge repository file packages/default/crates/cf-agent/src/deployment/agent.rs at commit 3b23d36f"
    title: Agent target handling
---

# Core Components

The one-page picture, with the meaning of every arrow, is in the
[Ecosystem architecture summary](../architecture/ecosystem-architecture-summary.md).
This page states what each component owns and which connections it starts.

## Server (Rust, `cf-server`)

- **Location:** central coordination node.
- **Owns:**
  - authoritative evaluation of flake commits with `nix-eval-jobs`, policy
    evaluation, and deployment decisions;
  - all PostgreSQL access;
  - authentication (local and OIDC sessions) and role-based authorization;
  - the evaluation and build queues, builder session and job authorization,
    and builder-liveness recovery;
  - CVE scan scheduling and evidence, compliance, and POA&M workflows;
  - the embedded Dioxus web UI assets.
- **Interfaces:**
  - `POST /agent/heartbeat` and `POST /agent/state` for signed agent reports.
    The response can carry a `desired_target`.
  - `/api/v1/builders/...` for API-only builders: signed, session-checked
    registration, heartbeat, job claim, log, completion, and scan routes.
  - `/api/v1/...` for the web UI and for administration.
  - `POST /webhook` for Git push events.
  - The embedded UI, served for every non-API path.
- **Connections it starts:** Git remotes (flake polling and source
  materialization), binary caches (a probe that confirms a builder-reported
  path), and outbound notification email.

## Builder (Rust, `cf-builder`)

- **Location:** one or more build hosts.
- **Owns:**
  - realizing derivations with Nix (`nix-store --realise`); Nix resolves
    dependencies;
  - publishing build outputs to the configured binary cache and reporting the
    cache reference;
  - streaming logs and heartbeats;
  - builder-side CVE scans that it claims through the API;
  - verified-source re-evaluation when `source_re_evaluate_verified` is
    configured. This verifies the server-authorized build plan; it does not
    replace authoritative server evaluation.
- **Does not own:** database access, authoritative evaluation, or policy.
- **Connections it starts:** every connection to the server (polling and
  heartbeat), to the binary cache, and, depending on the execution strategy,
  to Git or the substituters needed to realize the derivation.

## Agent (Rust, `cf-agent`)

- **Location:** each managed NixOS system.
- **Owns:**
  - detecting configuration changes and collecting a host fingerprint;
  - sending Ed25519-signed heartbeat and state reports;
  - reading `desired_target` from the server's response, pulling the closure
    from the cache, and activating it locally;
  - reporting deployment start and failure.
- **Does not own:** choosing the target.
- **Connections it starts:** to the server and to the binary cache.

## Web UI (Dioxus, `packages/web-ui`)

- **Location:** browser, loaded from the server.
- **Owns:** presentation and interaction. Every read and mutation goes through
  the server API, which authorizes it.
- **Does not own:** persistence, authorization decisions, or any direct
  connection to PostgreSQL, builders, or agents.

## Related concepts

- [Ecosystem architecture summary](../architecture/ecosystem-architecture-summary.md) - the one-page diagram
- [System overview](../overview/system-overview.md) - product-level overview
- [Data flows](../architecture/data-flows.md) - how data moves between these components
- [Wakeups and polling](../architecture/event-driven-queues.md) - how the server and builders discover queued work
- [ADR-000 Architecture overview](../decisions/adr-000-architecture-overview.md) - the decision record these components came from

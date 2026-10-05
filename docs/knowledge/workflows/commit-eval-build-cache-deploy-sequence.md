---
type: Workflow
title: "Commit -> Eval -> Build -> Cache -> Deploy Sequence"
description: "Shows the sequence diagram and reading guide for commit ingestion, evaluation, builder polling and atomic claim, builder-side cache publication with server verification, heartbeat-based recovery, and policy-gated deployment convergence; open it to see ordering, recovery, and the deployable-artifact rule."
tags:
  - crystal-forge
  - workflow
  - sequence
  - cache
  - deployment
implementation_status: implemented
generated:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-04T18:40:00-05:00
sources:
  - id: origin
    resource: "Crystal Forge repository file docs/eval-build-deploy-sequence.md at commit 3b23d36f"
    title: "Commit -> Eval -> Build -> Cache -> Deploy Sequence (original document)"
  - id: origin-2
    resource: "Crystal Forge repository file backlog/docs/doc-3%20-%20Commit-Eval-Build-Cache-Deploy-Sequence.md at commit 3b23d36f"
    title: Older copy with the earlier definition of deployable
  - id: code-1
    resource: "Crystal Forge repository file packages/default/crates/cf-server/src/bin/server.rs at commit 3b23d36f"
    title: Route table
  - id: code-2
    resource: "Crystal Forge repository file packages/default/crates/cf-server/src/handlers/api/builders.rs at commit 3b23d36f"
    title: Claim, complete, and fail handlers
  - id: code-3
    resource: "Crystal Forge repository file packages/default/crates/cf-agent/src/deployment/agent.rs at commit 3b23d36f"
    title: Agent heartbeat response handling and deployment
---

# Commit -> Eval -> Build -> Cache -> Deploy Sequence

This page shows who talks to whom and in what order. The
[flow chart](commit-eval-build-cache-deploy-flow.md) shows the same pipeline as
decisions and outcomes.

```mermaid
sequenceDiagram
    autonumber
    participant Src as Flake poller, API, or webhook
    participant S as Server (REST API and background tasks)
    participant DB as PostgreSQL
    participant NEJ as nix-eval-jobs
    participant B as Builder (cf-builder)
    participant C as Binary cache
    participant A as CF Agent

    rect rgb(240, 250, 240)
        note right of Src: Commit ingestion
        Src->>DB: Insert commit (evaluation pending, queued attempt)
        opt flake poll or API action
            Src-->>S: wake evaluation loop (coalesced, in process)
        end
        note over Src,S: A webhook inserts the commit and returns 202. It sends no wakeup.
    end

    rect rgb(230, 240, 255)
        note right of S: Evaluation loop (inside the server)
        S->>DB: Select next eligible commit (eval_queue_position, timestamp, id)
        S->>DB: Mark commit in progress
        S->>NEJ: Evaluate all nixosConfigurations in parallel
        NEJ-->>S: Per-system results and expected store paths
        S->>DB: Store derivations and policy outcomes
        S->>DB: Mark derivation dry-run-complete when evaluation and policy pass
        S->>DB: Create build jobs (queued) for those derivations
        S->>DB: Mark commit complete, failed, or pending retry
    end

    loop Builder polling (builder.poll_interval)
        B->>S: GET or POST /api/v1/builders/:id/next-job (signed, session id)
        S->>DB: claim_next_job_atomic (environment, capacity, eligibility)
        DB-->>S: job or none
        S-->>B: job manifest or no work
    end

    rect rgb(255, 245, 230)
        note right of B: Build
        B->>B: Build the derivation
        B->>S: POST /api/v1/builders/:id/jobs/:job_id/logs
        S->>DB: Append build logs
        B->>S: POST /api/v1/builders/:id/heartbeat (periodic)
    end

    alt Build and cache push succeed
        B->>B: Sign output (a failure is logged and ignored)
        B->>C: Push store path with retry
        C-->>B: Stored
        B->>S: POST /api/v1/builders/:id/jobs/:job_id/complete (store path and cache reference)
        S->>DB: Validate cache destination, complete job and derivation
        S->>C: nix path-info for the store path
        C-->>S: Path found
        S->>DB: Record cache push completed
        S-->>B: 200 OK
    else Build or cache push fails
        B->>S: POST /api/v1/builders/:id/jobs/:job_id/fail (phase and class)
        S->>DB: Record failure and queue a retry when the policy allows
        S-->>B: 200 OK when retried, 202 when no retry
    end

    note over S,DB: If a builder stops sending heartbeats, the recovery loop marks it offline and re-queues its building jobs.

    rect rgb(240, 230, 250)
        note right of S: Deployment convergence
        loop Deployment policy manager (every deployment_poll_interval)
            S->>DB: Select auto_latest systems
            S->>DB: Newest deployable artifact per configuration
            S->>DB: Evaluate policy gates, then authorize and set desired_target
        end
        A->>S: POST /agent/heartbeat (current system)
        S-->>A: Response with desired_target (or none) and heartbeat interval
        alt desired_target differs from current system
            A->>C: nix copy from cache
            A->>A: switch-to-configuration
            A->>S: POST /agent/state
            S->>DB: Insert system state record
        else already on target
            note over A,S: No action
        end
    end
```

## Reading guide

### Key architectural decisions

1. **The server owns all database writes.** A builder never opens a database
   connection. It reads and writes state through signed API requests, and the
   server applies authorization and idempotency rules. The evaluation loop and
   the deployment policy manager run inside the server process.

2. **Builders pull work.** `claim_next_job_atomic` gives a job to one builder
   with `FOR UPDATE ... SKIP LOCKED`. It also enforces the builder session,
   the builder capacity, and the environment match. Build jobs have **no
   lease expiry**. When a builder stops sending heartbeats, the recovery loop
   marks it `offline` after `max(3 x heartbeat interval, 60 s)` and re-queues
   its `building` jobs. A builder must tolerate the same job reaching another
   builder. See [Builder architecture](../builders/builder-architecture-and-job-scheduling.md).

3. **Evaluation produces partial results.** A commit can be `complete` while
   only some systems have a `dry-run-complete` derivation. The server creates
   build jobs only for derivations that passed evaluation and policy.

4. **Evaluation policy and deployment policy differ.**
   - **Evaluation policy** decides whether a system may be built (for example,
     the CF agent must be enabled).
   - **Deployment policy** decides which build a host runs (`auto_latest`,
     `manual`, `pinned`). Policy gates (for example CVE checks, time windows,
     and canary rollout) run before the server sets `desired_target`.

5. **The builder publishes to the cache.** The server does not run a cache
   worker. A job fails when the builder cannot push. The server then verifies
   the reported path with `nix path-info` and records the completed push. See
   [Cache push process](../caches/cache-push-process.md).

6. **Deployable system artifact.** For a host's registered flake and effective
   configuration, an eligible artifact is a NixOS derivation with a nonblank
   `store_path`, `cf_agent_enabled IS TRUE`, `policy_requirements_met IS TRUE`,
   and no derivation error. A completed `cache_push_jobs` row must belong to
   that derivation and have `store_path = derivations.store_path`. Auto-latest
   considers retained built artifacts across commits, even if source archival
   prevents a new explicit manual commit request. Runtime deployment policies
   and final authorization are separate gates. An eligible artifact does not
   authorize delivery to a host by itself.

7. **GC roots.** After evaluation, the server creates a GC root for each
   evaluated `.drv` file so that a builder can download it before Nix garbage
   collection removes it. The server replaces such a root only when it creates
   the root again. The builder creates a GC root for its build output on the
   builder host (`nix-store --realise --add-root`). When the server records a
   completed cache push, it removes only the server-local output root
   (`derivation-<id>`). No step of this flow removes the builder-host root.

8. **`desired_target` is per host.** The policy manager sets it for each
   `auto_latest` system. Different hosts can run different commits. Manual and
   pinned policies keep a host on a chosen target.

## Earlier definition of deployable

> **Status:** historical. This text is item 6 of the reading guide in the
> Backlog.md copy doc-3. Item 6 above supersedes it. The earlier definition
> omitted the exact-store-path match and the artifact eligibility conditions.

A derivation is deployable when:

- build status is success;
- cache push status is completed (the artifact is in the binary cache);
- (implicit) policy allows deployment to that host.

## Related concepts

- [Commit to deploy flow](commit-eval-build-cache-deploy-flow.md) - flow chart of the same pipeline
- [Evaluation and build queue pipeline](evaluation-and-build-queue-pipeline.md) - database fields behind evaluation and build
- [Deployment flow](../deployment/deployment-flow.md) - agent side of the convergence step
- [Derivation status lifecycle](../concepts/derivation-status-lifecycle.md) - statuses the diagram refers to

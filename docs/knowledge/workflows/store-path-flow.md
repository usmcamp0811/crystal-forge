---
type: Workflow
title: "Crystal Forge store path flow"
description: "Shows how Crystal Forge records an expected store path per nixosConfiguration at evaluation, sets the built store path at build completion, requires a completed cache push for a deployable artifact, and classifies each system as up_to_date, behind, ahead, unknown, or no_deployment; open it to understand per-system store path tracking."
tags:
  - crystal-forge
  - workflow
  - store-path
  - evaluation
  - agent
implementation_status: implemented
generated:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-04T19:20:00-05:00
sources:
  - id: origin
    resource: "Crystal Forge repository file docs/store-path-flow.md at commit 3b23d36f"
    title: "Crystal Forge Store Path Flow (original document)"
  - id: code-1
    resource: "Crystal Forge repository file packages/default/crates/cf-server/migrations/0291_prefer_deployable_running_path_identity.sql at commit 3b23d36f"
    title: Current view_system_deployment_status definition
  - id: code-2
    resource: "Crystal Forge repository file packages/default/crates/cf-server/src/queries/derivations.rs at commit 3b23d36f"
    title: Deployable artifact selection
  - id: code-3
    resource: "Crystal Forge repository file packages/default/crates/cf-server/src/server/mod.rs at commit 3b23d36f"
    title: Evaluation finalization and build job creation
---

# Crystal Forge Store Path Flow

This page shows how Crystal Forge tracks the store path of each system from
evaluation to deployment. An arrow means "then". A cylinder is a database
write.

## Two store path columns

A `derivations` row has two store path columns:

| Column | Written when | Meaning |
| --- | --- | --- |
| `expected_store_path` | Evaluation (from the `nix-eval-jobs` outputs) | The path the system will have when built |
| `store_path` | Build completion (`POST .../jobs/:job_id/complete`) | The path a builder reported as built |

A system is deployable only when `store_path` is set **and** a completed cache
push row exists for that exact path.

## The complete flow

```mermaid
flowchart TB
    subgraph S1["1. Commit arrives"]
        COMMIT[New flake commit]
    end

    subgraph S2["2. Evaluation (parallel per nixosConfiguration)"]
        COMMIT --> EVAL[nix-eval-jobs evaluates every configuration]
        EVAL --> P1[(Save expected_store_path<br/>per configuration)]
        P1 --> FIN[Commit finalization]
        FIN --> JOBS[(Create build jobs<br/>for derivations that passed policy)]
    end

    subgraph S3["3. Build queue"]
        JOBS --> QUEUE[Queued by queue_position<br/>newest batch first]
        QUEUE --> BUILD[Builder claims job and builds]
    end

    subgraph S4["4. Cache publication"]
        BUILD --> PUSH[Builder signs and pushes to cache]
        PUSH --> REPORT[Builder reports store path and cache reference]
        REPORT --> PROBE[Server probes cache with nix path-info]
        PROBE --> DONE[(Save store_path<br/>and completed cache push)]
    end

    subgraph S5["5. Deployment and status"]
        DONE --> TARGET[Newest deployable artifact<br/>per configuration]
        TARGET --> DPM[Policy manager sets desired_target<br/>auto_latest only, after policy gates]
        DPM --> HB[Agent heartbeat response carries desired_target]
        HB --> ACT[Agent copies from cache and switches]
        ACT --> REP[Agent reports current system path]
        REP --> VIEW[view_system_deployment_status<br/>compares current path with newest deployable]
        VIEW --> ST{Classification}
        ST --> UP[up_to_date]
        ST --> BEH[behind]
        ST --> AHD[ahead]
        ST --> UNK[unknown]
        ST --> NODEP[no_deployment]
    end
```

## Key points

### Evaluation is parallel, and builds start after finalization

- `nix-eval-jobs` evaluates the configurations of one commit in parallel. The
  server persists each system as its result arrives and stores its
  `expected_store_path`.
- The server creates the build jobs when it **finalizes** the commit
  (`EvaluationFinalizeOutcome::Completed` in `server/mod.rs`). A build does
  not start while another system of the same commit is still evaluating.
- After the build jobs exist, each system builds and publishes independently.
  One builder can build `hostname1` while another publishes `hostname3`.

### Build order is newest batch first

The claim query sorts by `queue_position DESC`. New jobs receive a position
higher than every queued or building job. The newest batch is therefore claimed
first unless an operator reorders the queue. Within a batch the later
derivation id has the higher position. This order is not a FIFO. It is also
not a strict LIFO per commit. See
[Wakeups and polling](../architecture/event-driven-queues.md#claim-eligibility-and-ordering).

### Deployable artifact

The newest deployable artifact for a configuration is the first row, by commit
timestamp, completion time, and id, of a NixOS derivation that has:

- a nonblank `store_path`;
- `cf_agent_enabled` and `policy_requirements_met` set;
- no error message;
- a completed `cache_push_jobs` row with the same `store_path`.

### System states

The server computes the state in `view_system_deployment_status` (migration
`0291`). The agent does not classify itself. The agent only sends its current
system path and receives `desired_target`.

| State | Meaning |
| --- | --- |
| `no_deployment` | The system is registered, but no system state record exists for its hostname. |
| `up_to_date` | The current path equals the newest deployable artifact of the system. |
| `behind` | The current path belongs to the system's own configuration but not to the newest deployable artifact. A newer deployable build is available. |
| `ahead` | The current path belongs to a newer commit than the newest deployable artifact. |
| `unknown` | The server cannot relate the current path to the system's flake and configuration, or the configuration has no deployable artifact yet. |

A path that appears in no `derivations` row (neither `store_path` nor
`expected_store_path`) falls into `unknown`.

### The loop

1. **Commit arrives.** The server evaluates each configuration.
2. **Evaluation persists.** Each system's expected store path is saved as the
   system completes.
3. **Finalization queues builds.** Systems that passed policy get build jobs.
4. **Build and publish.** A builder builds, pushes to the cache, and reports.
5. **Server verifies.** The server probes the cache and records the completed
   push and the built `store_path`.
6. **Policy manager sets the target.** For an `auto_latest` system, after the
   policy gates pass.
7. **Agent heartbeats.** The heartbeat response carries `desired_target`. The
   agent copies the path from the cache and switches.
8. **Agent reports.** The next state report carries the new path. The view
   then shows `up_to_date`.

### Why this design

- **Parallel evaluation** gives fast feedback on which configurations change.
- **The cache requirement** prevents the server from pointing an agent at a
  path that no cache can serve.
- **Pull-based deployment** keeps the agent in control of when it switches. The
  server never pushes to an agent.
- **A server-computed state** gives one definition of "up to date" for every
  view.

## Related concepts

- [Commit to deploy flow](commit-eval-build-cache-deploy-flow.md) - flow chart with decisions
- [Commit to deploy sequence](commit-eval-build-cache-deploy-sequence.md) - sequence diagram
- [Evaluation and build queue pipeline](evaluation-and-build-queue-pipeline.md) - queue and invariant details
- [System Deployment Status View](../data-model/views/view-system-deployment-status.md) - the view that classifies systems

---
type: Workflow
title: "Commit -> Eval -> Build -> Cache -> Deploy Flow"
description: "Shows the flow chart of how a commit moves from discovery through evaluation, build, cache push, and pull-based deployment; open it for a one-page picture of the pipeline."
tags:
  - crystal-forge
  - workflow
  - flow
  - evaluation
  - build
  - cache
  - deployment
implementation_status: implemented
sources:
  - id: origin
    resource: "Crystal Forge repository file docs/eval-build-deploy-flow.md at commit 3b23d36f"
    title: "Commit -> Eval -> Build -> Cache -> Deploy Flow"
---

# Commit -> Eval -> Build -> Cache -> Deploy Flow

> **Status:** The Backlog.md document doc-2 held a content-identical copy. That document now points to this concept. The flow has not been compared in full with the implementation. `QueueNotifier` exists in `packages/default/crates/cf-server/src/queue/mod.rs`. The step "Notify build queue" sends a wakeup, but no build worker waits on it yet (see [Event-driven queue architecture](../architecture/event-driven-queues.md)).

This diagram explains how Crystal Forge processes a commit from discovery to deployment using the event-driven queue flow.

## Mermaid Flowchart

```mermaid
flowchart TD
    A[Commit detected] --> B[Insert commit pending]
    B --> C[Notify eval queue]
    C --> D[Eval loop wake up]

    D --> E{Pending commit exists}
    E -->|yes| F[Pick by eval queue position]
    E -->|no| D

    F --> G[Mark commit in progress]
    G --> H[Run nix eval jobs]
    H --> I[Store derivation results]
    I --> J{Per system outcome}

    J -->|eval failed| K[System marked failed]
    J -->|policy failed| L[System marked policy failed]
    J -->|eval and policy pass| M[Mark derivation dry run complete]

    M --> N[Create build jobs for commit]
    N --> O[Build jobs queued]
    O --> P[Notify build queue]

    P --> Q[Builder claims next job]
    Q --> R[Run build]
    R --> S{Build outcome}

    S -->|success| T[Build complete with store path]
    S -->|failed| U[Build retry or fail]

    T --> V[Enqueue cache push job]
    V --> W[Cache worker claims cache job]
    W --> X{Cache push outcome}

    X -->|success| Y[Cache push completed]
    X -->|failed| Z[Cache push retry with backoff]

    Y --> AA[Deployable target available]
    Z --> AA

    AA --> AB[Policy manager updates desired target]
    AB --> AC[Agent heartbeat reads desired target]
    AC --> AD{Current target equals desired}
    AD -->|yes| AE[No deployment action]
    AD -->|no| AF[Agent deploys from cache]
    AF --> AG[Agent reports deployment state]
    AG --> AH[Fleet converges on new target]
```

## Quick Explanation

- Commits enter the eval queue and trigger immediate processing through `QueueNotifier`.
- Evaluation is commit-level, but system evaluation happens in parallel via `nix-eval-jobs`.
- Only systems that pass evaluation and policy become buildable derivations.
- Build jobs are claimed atomically by builders to avoid race conditions.
- Successful builds create cache-push jobs; cache push has retries with backoff.
- Deployment is pull-based: agents receive `desired_target` on heartbeat and converge to it.

## Related concepts

- [Commit to deploy sequence](commit-eval-build-cache-deploy-sequence.md) - who talks to whom, in order
- [Store path flow](store-path-flow.md) - per-system store path tracking
- [Evaluation and build queue pipeline](evaluation-and-build-queue-pipeline.md) - queue behavior in detail

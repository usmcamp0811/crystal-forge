---
type: Workflow
title: "Commit -> Eval -> Build -> Cache -> Deploy Flow"
description: "Shows the flow chart of how a commit moves from discovery through evaluation, builder polling and build, builder-side cache publication with server verification, and policy-gated pull-based deployment; open it for a one-page picture of the pipeline."
tags:
  - crystal-forge
  - workflow
  - flow
  - evaluation
  - build
  - cache
  - deployment
implementation_status: implemented
generated:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-04T18:20:00-05:00
sources:
  - id: origin
    resource: "Crystal Forge repository file docs/eval-build-deploy-flow.md at commit 3b23d36f"
    title: "Commit -> Eval -> Build -> Cache -> Deploy Flow (original document)"
  - id: code-1
    resource: "Crystal Forge repository file packages/default/crates/cf-builder/src/bin/builder.rs at commit 3b23d36f"
    title: Builder sign, cache push, and completion report
  - id: code-2
    resource: "Crystal Forge repository file packages/default/crates/cf-server/src/handlers/api/builders.rs at commit 3b23d36f"
    title: Completion handler and cache probe
  - id: code-3
    resource: "Crystal Forge repository file packages/default/crates/cf-server/src/deployment/mod.rs at commit 3b23d36f"
    title: Deployment policy manager
---

# Commit -> Eval -> Build -> Cache -> Deploy Flow

This page shows how Crystal Forge moves a commit from discovery to deployment.
An arrow means "then". A diamond is a decision. The
[sequence page](commit-eval-build-cache-deploy-sequence.md) shows which
component performs each step.

```mermaid
flowchart TD
    A[Commit detected: flake poll, API action, or webhook] --> B[Insert commit, evaluation pending]
    B --> C{Source}
    C -->|flake poll or API| D[Wake evaluation loop]
    C -->|webhook| E[No wakeup; wait for fallback tick or other wakeup]
    D --> F[Evaluation loop selects eligible commit]
    E --> F

    F --> G[Mark commit in progress]
    G --> H[Run nix-eval-jobs for each nixosConfiguration]
    H --> I[Store derivations and expected store paths]
    I --> J{Per system outcome}
    J -->|evaluation failed| K[System marked failed]
    J -->|policy failed| L[System marked policy failed]
    J -->|evaluation and policy pass| M[Derivation dry-run-complete]

    M --> N[Create build job, queued]
    N --> O[Builder polls next-job and claims atomically]
    O --> P[Builder runs the build]
    P --> Q{Build outcome}
    Q -->|failed| R[Fail job: retry per policy or fail permanently]
    Q -->|success| S[Builder signs output]
    S --> T{Cache publication configured}
    T -->|no| U[Fail job: cache push required]
    T -->|yes| V[Builder pushes to cache with retry]
    V --> W{Push outcome}
    W -->|failed| R
    W -->|success| X[Builder reports complete with cache reference]

    X --> Y[Server validates cache destination and probes store path]
    Y --> Z{Store path found}
    Z -->|no| R2[Respond 409: job already complete, no cache row, not deployable]
    Z -->|yes| AA[Record cache push completed]

    AA --> AB[Deployable artifact exists]
    AB --> AC[Deployment policy manager: auto_latest systems only]
    AC --> AD{Policy gates}
    AD -->|block or pending| AE[desired_target unchanged]
    AD -->|allow or warn| AF[Authorize and set desired_target]
    AF --> AG[Agent heartbeat response carries desired_target]
    AG --> AH{Running equals desired}
    AH -->|yes| AI[No deployment action]
    AH -->|no| AJ[Agent runs nix copy from cache, then switch-to-configuration]
    AJ --> AK[Agent reports state; fleet converges]
```

## How to read the flow

- **Evaluation wakeup.** The flake poller and API actions wake the evaluation
  loop. The webhook handler inserts the commit and returns `202 Accepted`
  without a wakeup. The commit then waits for the next wakeup or the fallback
  tick (`flakes.commit_evaluation_interval`, default 60 seconds). See
  [Wakeups and polling](../architecture/event-driven-queues.md).
- **Evaluation unit.** Evaluation is per commit. `nix-eval-jobs` evaluates the
  systems in parallel. Only a system that passes evaluation and policy gets a
  build job.
- **Build discovery.** Builders poll the server API. The server sends no push
  to a builder. The pickup delay is at most `builder.poll_interval` (default 5
  seconds) plus claim contention. Several builders can poll at once; the
  atomic claim gives each job to one builder.
- **Cache publication is part of the build.** The builder signs the output
  (a signing failure is logged and does not fail the job), then pushes it. If
  the builder has no usable cache configuration, the job fails. A push retries
  inside the builder (`max_retries`, `retry_delay_seconds`, and
  `push_timeout_seconds` of the cache configuration). A final push failure
  fails the job, and the retry policy then decides whether to queue another
  attempt.
- **Server verification.** The server rejects a reported push with `409` when
  the cache reference matches no active cache destination. This check runs
  before the job completes. After it, the server commits the job completion,
  then probes the store path with `nix path-info`. When the probe finds the
  path, the server records a completed `cache_push_jobs` row. When the probe
  fails, the server responds `409`. The job completion stays committed, but no
  completed cache row exists, so the artifact is not deployable. The completion
  call is idempotent for the same builder and session. The server does not run
  its own cache worker.
- **Deployable artifact.** An artifact is deployable only when it has a
  `store_path`, `cf_agent_enabled` and `policy_requirements_met`, no error, and
  a completed cache push row for the same store path.
- **Deployment is pull-based.** Deployment policy gates apply after an artifact
  is deployable. The agent only learns `desired_target` from the heartbeat
  response. The heartbeat interval comes from the server
  (`server.heartbeat_interval_secs`, default 600 seconds, allowed 15 to 900).
- **Manual and pinned systems.** The policy manager does not change a system
  that is not `auto_latest`.

## Related concepts

- [Commit to deploy sequence](commit-eval-build-cache-deploy-sequence.md) - who talks to whom, in order
- [Store path flow](store-path-flow.md) - per-system store path tracking
- [Evaluation and build queue pipeline](evaluation-and-build-queue-pipeline.md) - queue behavior in detail
- [Cache push process](../caches/cache-push-process.md) - cache publication and verification

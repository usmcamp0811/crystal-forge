# Commit -> Eval -> Build -> Cache -> Deploy Flow

This diagram explains how Crystal Forge processes a commit from discovery to deployment using the event-driven queue flow.

## Mermaid Flowchart

```mermaid
flowchart TD
    A[New commit detected\nflake poll/webhook/manual] --> B[Commit inserted in DB\nstatus=pending]
    B --> C[QueueNotifier.notify_eval_queue]
    C --> D[Eval loop wakes immediately\nwith periodic fallback tick]

    D --> E{Pick next commit}
    E -->|ordered by eval_queue_position,\nthen commit time| F[Mark commit in_progress\n(single active eval enforced)]
    E -->|none| D

    F --> G[nix-eval-jobs evaluates all systems\nin parallel]
    G --> H[Policy checks per system\nCF agent enabled?]
    H --> I[Store/Update derivations]
    I --> J{System outcome}

    J -->|eval failed| K[System status=Failed]
    J -->|policy failed| L[System status=PolicyFailed]
    J -->|eval ok + policy pass| M[Mark derivation DryRunComplete\nstatus_id=5]

    M --> N[create_build_jobs_for_commit]
    N --> O[Build jobs inserted\nstatus=queued]
    O --> P[QueueNotifier.notify_build_queue]

    P --> Q[Builder claims next job\natomic claim + capacity limits]
    Q --> R[Build execution]
    R --> S{Build result}

    S -->|success| T[Mark build complete\nstore_path saved]
    S -->|failure| U[Retry or terminal fail\nbased on retry policy]

    T --> V[Create cache_push_job]
    V --> W[Cache worker picks job]
    W --> X{Cache push result}

    X -->|success| Y[cache_push_job completed\nremove GC root]
    X -->|failed| Z[Backoff retry\nthen permanently_failed after max attempts]

    Y --> AA[Deployability improved\nlatest deployable target available]
    Z --> AA

    AA --> AB[Deployment Policy Manager loop\nauto_latest systems]
    AB --> AC[Update system desired_target\nto latest deployable store path]

    AC --> AD[Agent heartbeat to server\nreturns desired_target]
    AD --> AE{Agent compares\ncurrent vs desired}
    AE -->|already current| AF[No deployment]
    AE -->|different| AG[Deploy from cache\nnix copy + switch via systemd-run]
    AG --> AH[Agent reports state change\ncf_deployment]
    AH --> AI[Fleet converges on newer target]
```

## Quick Explanation

- Commits enter the eval queue and trigger immediate processing through `QueueNotifier`.
- Evaluation is commit-level, but system evaluation happens in parallel via `nix-eval-jobs`.
- Only systems that pass evaluation and policy become buildable derivations.
- Build jobs are claimed atomically by builders to avoid race conditions.
- Successful builds create cache-push jobs; cache push has retries with backoff.
- Deployment is pull-based: agents receive `desired_target` on heartbeat and converge to it.

# Commit -> Eval -> Build -> Cache -> Deploy Sequence

This sequence diagram shows who talks to whom and in what order.

## Mermaid Sequence Diagram

```mermaid
sequenceDiagram
    autonumber
    participant Poller as Flake Poller/Webhook
    participant API as Crystal Forge Server
    participant EQ as Eval Loop
    participant NEJ as nix-eval-jobs
    participant DB as PostgreSQL
    participant B as Builder
    participant CQ as Cache Worker
    participant DPM as Deployment Policy Manager
    participant Agent as CF Agent

    Poller->>API: New commit detected
    API->>DB: Insert commit (evaluation_status=pending)
    API->>EQ: notify_eval_queue()

    EQ->>DB: Select next pending commit by eval_queue_position
    EQ->>DB: Mark commit in_progress
    EQ->>NEJ: Evaluate all nixosConfigurations in parallel
    NEJ-->>EQ: Per-system eval results + metadata

    EQ->>DB: Upsert derivations + policy outcomes
    EQ->>DB: Mark successful derivations DryRunComplete
    EQ->>DB: Mark commit complete or failed/pending(retry)

    EQ->>DB: create_build_jobs_for_commit()
    EQ->>B: notify_build_queue()

    loop Build queue processing
        B->>API: GET /builders/:id/next-job
        API->>DB: Atomic claim (respect capacity/env)
        DB-->>API: build job or none
        API-->>B: Claimed job
        B->>DB: Build derivation, update status/logs
        alt Build success
            B->>DB: Mark build complete + store_path
            B->>DB: Create cache_push_job
        else Build failed
            B->>DB: mark_job_failed_with_retry()
        end
    end

    loop Cache push processing
        CQ->>DB: Claim pending/retryable cache_push_job
        CQ->>DB: Mark cache job in_progress
        CQ->>CQ: Push to configured cache backend
        alt Push success
            CQ->>DB: Mark cache job completed
            CQ->>DB: Remove GC root
        else Push failed
            CQ->>DB: Mark failed + retry_after backoff
        end
    end

    loop Deployment convergence
        DPM->>DB: Find auto_latest systems
        DPM->>DB: Compute latest deployable targets per host
        DPM->>DB: Update desired_target for each system

        Agent->>API: Heartbeat + current state
        API->>DB: Fetch desired_target
        API-->>Agent: desired_target (or none)

        alt desired_target differs from current
            Agent->>Agent: nix copy from cache + activate via systemd-run
            Agent->>API: Report cf_deployment state change
        else already current
            Agent->>API: Heartbeat only
        end
    end
```

## Reading Guide

- `notify_eval_queue()` reduces wait time by waking eval processing immediately.
- Eval ordering is user-controllable through queue position.
- Builders and cache workers are separate stages with separate retry behavior.
- Deployment is driven by `desired_target` updates and agent heartbeats.

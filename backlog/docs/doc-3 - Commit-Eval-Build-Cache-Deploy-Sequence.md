---
id: doc-3
title: Commit -> Eval -> Build -> Cache -> Deploy Sequence
type: other
created_date: '2026-03-03 15:32'
---
# Commit -> Eval -> Build -> Cache -> Deploy Sequence

This sequence diagram shows who talks to whom and in what order.

## Mermaid Sequence Diagram

```mermaid
sequenceDiagram
    autonumber
    participant Poller as Flake Poller / Webhook
    participant API as Crystal Forge Server (REST API)
    participant DB as PostgreSQL
    participant EQ as Eval Worker
    participant NEJ as nix-eval-jobs
    participant B as Builder
    participant CQ as Cache Worker
    participant DPM as Deployment Policy Manager
    participant Agent as CF Agent

    rect rgb(240, 250, 240)
        note right of Poller: Commit ingestion
        Poller->>API: New commit detected
        API->>DB: Insert commit (status=pending)
        API->>EQ: notify_eval_queue()
    end

    rect rgb(230, 240, 255)
        note right of EQ: Eval loop (continuous)
        EQ->>DB: SELECT next pending commit (by eval_queue_position)
        EQ->>DB: UPDATE commit SET status=in_progress
        EQ->>NEJ: Evaluate all nixosConfigurations in parallel
        NEJ-->>EQ: Per-system eval results + metadata

        EQ->>DB: UPSERT derivations + eval_policy outcomes
        EQ->>DB: UPDATE derivation SET status=DryRunComplete (if eval+policy passed)
        EQ->>DB: UPDATE commit SET status=complete/failed/pending_retry
    end

    EQ->>DB: INSERT build_jobs (one per derivation with DryRunComplete)
    EQ->>B: notify_build_queue()

    loop Build queue (with lease semantics)
        B->>API: GET /builders/:id/next-job
        API->>DB: Atomic claim (respects max_concurrent_jobs)
        DB-->>API: job + lease_expires_at
        API-->>B: Job with lease

        rect rgb(255, 245, 230)
            note right of B: Building (lease renewal)
            B->>B: Execute nix build
            B->>API: POST /builders/:id/jobs/:id/logs (streaming)
            API->>DB: Append build logs
        end

        alt Build success
            B->>API: POST /builders/:id/jobs/:id/complete
            API->>DB: UPDATE job SET status=success, store_path
            API->>DB: INSERT cache_push_job (for this derivation)
            API->>B: 200 OK
            note right of B: Builder creates GC root after build
        else Build failed
            B->>API: POST /builders/:id/jobs/:id/fail
            API->>DB: UPDATE job SET retry_count+1, status=queued/failed
            API->>B: 200 OK (or 202 if permanently failed)
        end

        Note over B,DB: If builder dies: lease expires, job re-claimable by another
    end

    rect rgb(250, 240, 250)
        note right of CQ: Cache push (idempotent)
        loop Cache worker (processes pending jobs)
            CQ->>DB: SELECT cache_push_job WHERE status=pending (order by created_at)
            CQ->>DB: UPDATE job SET status=in_progress, attempts=attempts+1
            CQ->>CQ: nix copy --to cache-backend (idempotent: "exists" = success)
            alt Push success
                CQ->>DB: UPDATE job SET status=completed
                CQ->>DB: DELETE gc_root (artifact now in cache, safe to collect)
            else Push failed
                CQ->>DB: UPDATE job SET status=failed, retry_after=now()+backoff
            end
        end
    end

    rect rgb(240, 230, 250)
        note right of DPM: Deployment convergence
        loop Policy manager (periodic)
            DPM->>DB: SELECT systems WHERE deployment_policy=auto_latest
            DPM->>DB: For each host: latest derivation WHERE status=success AND cache_status=completed
            DPM->>DB: UPDATE system SET desired_target=:latest_deployable_store_path
        end

        Agent->>API: POST /agent/heartbeat (includes /run/current-system)
        API->>DB: SELECT system.desired_target
        API-->>Agent: desired_target (or null)

        alt desired_target != current_system
            Agent->>Agent: nix copy --from cache + systemctl switch
            Agent->>API: POST /agent/state (state_change_reason=cf_deployment)
            API->>DB: INSERT system_state record
        else already current
            Note over Agent,API: No action needed
        end
    end
```

## Reading Guide

### Key Architectural Decisions

1. **All DB writes go through API**: Builders, eval workers, and cache workers never write to DB directly. This provides authentication, authorization, and a central place for idempotency logic.

2. **Job lease semantics**: When a builder claims a job, it receives a lease (`lease_expires_at`). If the builder dies, the lease eventually expires and the job can be re-claimed by another builder (at-least-once delivery). Builders should be idempotent when re-running work.

3. **Eval produces partial results**: A commit can be "complete" but only some systems may have `DryRunComplete` derivations. Build jobs are only created for derivations that passed both evaluation AND policy checks.

4. **Eval policy vs Deployment policy**:
   - **Eval policy** (checked during eval): Is this system allowed to be built? (e.g., CF agent must be enabled)
   - **Deployment policy** (checked by DPM): Which version should this host run? (e.g., auto_latest, manual, pinned)

5. **Cache push is idempotent**: Pushing the same store path twice is safe - the cache backend returns "already exists" which we treat as success.

6. **"Deployable" definition**: A derivation is deployable when:
   - Build status = success
   - Cache push status = completed (artifact in binary cache)
   - (Implicit) Policy allows deployment to that host

7. **GC root lifecycle**: Builder creates a GC root after successful build to prevent Nix from garbage-collecting the output before it reaches the cache. Cache worker removes the GC root only after successful push.

8. **desired_target is per-host**: The DPM sets `desired_target` individually for each system, allowing different hosts to be on different commits ("partial rollouts" or "canary deployments" are supported via manual/pinned policies).

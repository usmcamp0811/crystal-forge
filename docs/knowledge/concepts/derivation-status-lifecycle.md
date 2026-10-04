---
type: Concept
title: "Derivation status lifecycle"
description: "Defines the derivation status IDs and names, the combined lifecycle sequence, terminal states, and retry rules; open it when reading or changing derivation status handling."
tags:
  - crystal-forge
  - concept
  - derivation
  - status
  - retry
implementation_status: partial
generated:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-03T22:57:15-05:00
sources:
  - id: origin
    resource: "Crystal Forge repository file docs/derivation-status.md at commit 3b23d36f"
    title: "Crystal Forge Derivation Status Flow"
---

# Crystal Forge Derivation Status Flow

This document explains the statuses that derivations go through in Crystal Forge and how they transition during processing, including deployment and cache operations.

> **Status:** partial. IDs 1 to 13 match the seeded `derivation_statuses` rows (`packages/default/crates/cf-server/migrations/0026_make_derivation_statuses.sql`). The database names are `dry-run-inprogress`, `build-inprogress`, and `in-progress`, while this document writes `dry-run-in-progress`, `build-in-progress`, and `in-progress (legacy)`. The seed also marks `dry-run-failed` and `build-failed` terminal without the retry exception, and does not seed `cache-pushed (14)` in that migration. These are verification candidates. `reset_non_terminal_derivations()` exists in `packages/default/crates/cf-server/src/queries/derivations.rs`.

## Combined Lifecycle (Sequence)

```mermaid
sequenceDiagram
    participant Commit as New Commit
    participant Eval as Evaluation Loop
    participant Build as Build Loop
    participant Cache as Cache Push Loop
    participant CVE as CVE Scanner
    participant Deploy as Deployment Manager
    participant Agent as Agent
    participant DB as Database

    %% Commit discovery
    Commit->>DB: Insert NixOS derivations
    Note over DB: Set status = (dry-run-pending)

    %% Evaluation loop
    Eval->>DB: Find status = dry-run-pending
    DB-->>Eval: Return rows
    Eval->>DB: Update → (dry-run-in-progress)
    Eval->>Eval: nix build --dry-run

    alt Evaluation succeeded
        Eval->>DB: Update → (dry-run-complete)
        Eval->>DB: Insert discovered package deps
        Note over DB: Packages initially at (build-pending)
    else Evaluation failed
        Eval->>DB: Update → (dry-run-failed)
    end

    %% Build loop
    Build->>DB: Find status IN (dry-run-complete,build-pending)
    DB-->>Build: Return rows
    Build->>DB: Update → (build-in-progress)
    Build->>Build: nix-store --realise

    alt Build succeeded
        Build->>DB: Update → (build-complete)
        
        %% Cache push flow
        Cache->>DB: Find derivations needing cache push
        Cache->>DB: Create cache push job
        Cache->>DB: Mark cache job in-progress
        Cache->>Cache: Push to cache (S3/Attic/Nix)
        
        alt Cache push succeeded
            Cache->>DB: Mark cache job completed
            Cache->>DB: Update → (cache-pushed)
        else Cache push failed
            Cache->>DB: Mark cache job failed
            Note over Cache: Retries with exponential backoff
        end
        
    else Build failed
        Build->>DB: Update → (build-failed)
    end

    %% CVE Scanning
    CVE->>DB: Find build-complete derivations
    CVE->>CVE: Run vulnix scan
    CVE->>DB: Save CVE scan results
    
    %% Deployment Policy Management
    Deploy->>DB: Find systems with auto_latest policy
    Deploy->>DB: Get latest successful derivation per flake
    Deploy->>DB: Update system desired_target
    
    %% Agent Deployment Flow
    Agent->>Agent: Heartbeat to server
    Agent-->>Deploy: Receive desired_target
    
    alt Agent needs deployment
        Agent->>Agent: Check if same derivation path
        alt Different derivation path
            Agent->>Agent: Execute deployment (nixos-rebuild)
            alt Deployment succeeded
                Agent->>DB: Report new system state
                Note over Agent: change_reason = "cf_deployment"
            else Deployment failed
                Agent->>DB: Report deployment failure
            end
        else Same derivation path
            Note over Agent: Skip deployment - already current
        end
    else No deployment needed
        Agent->>DB: Regular heartbeat
        Note over Agent: change_reason = "heartbeat"
    end

    %% Retry handler
    loop While attempt_count < 5
        Retry->>DB: Reset (dry-run-failed → pending)
        Retry->>DB: Reset (build-failed → build-pending)
    end
    Note over DB: attempt_count >= 5 → terminal

    %% Alternative entry via CVE scanning
    CVE->>DB: Insert packages from scan
    Note over DB: Set status = (complete)
```

## Status Table

|  ID | Name                 | Description               | Terminal | Next Step |
| --: | -------------------- | ------------------------- | -------- | --------- |
|   1 | pending              | Should not be used        | ❌       | → dry-run-pending |
|   2 | queued               | Reserved for future use   | ❌       | → dry-run-pending |
|   3 | dry-run-pending      | Ready for dry-run         | ❌       | → evaluation loop |
|   4 | dry-run-in-progress  | Running nix dry-run       | ❌       | → dry-run-complete/failed |
|   5 | dry-run-complete     | Dry-run succeeded         | ❌       | → build loop |
|   6 | dry-run-failed       | Dry-run failed            | ✅\*     | → retry or terminal |
|   7 | build-pending        | Ready for build           | ❌       | → build loop |
|   8 | build-in-progress    | Building                  | ❌       | → build-complete/failed |
|   9 | in-progress (legacy) | Generic in-progress       | ❌       | → legacy handling |
|  10 | build-complete       | Build succeeded           | ❌       | → cache push |
|  11 | complete             | Fully complete (packages) | ✅       | → CVE scanning |
|  12 | build-failed         | Build failed              | ✅\*     | → retry or terminal |
|  13 | failed               | Generic failure           | ✅       | N/A |
|  14 | cache-pushed         | Pushed to binary cache    | ✅       | → CVE scanning |

\* Terminal only if maximum retry attempts reached.

## Retry Logic

### **Automatic Retries**
- **Max attempts:** 5 (configurable)
- **Reset conditions:**
  - `dry-run-failed` → `dry-run-pending` if attempts < 5
  - `build-failed` → `build-pending` if attempts < 5
- **Reset trigger:** `reset_non_terminal_derivations()` on startup
- **Backoff:** Exponential delay between retries for cache operations

### **Manual Intervention**
- Reset attempt count to force retry
- Update derivation target for different commit
- Modify build configuration for resource issues

## Terminal States

### **Successful Completion**
- **build-complete (10)**: Ready for cache push and CVE scanning
- **cache-pushed (14)**: Successfully cached, ready for deployment
- **complete (11)**: Fully processed (typically for packages)

### **Failure States**
- **dry-run-failed (6)**: Configuration invalid, requires code fix
- **build-failed (12)**: Build errors, may need dependency updates
- **failed (13)**: Generic failure state

## Related concepts

- [Derivation processing loops](../architecture/derivation-processing-loops.md) - the loops that move derivations between statuses
- [Cache push process](../caches/cache-push-process.md) - the cache-pushed transition
- [Evaluation and build queue pipeline](../workflows/evaluation-and-build-queue-pipeline.md) - status IDs in the two-stage pipeline
- [Observability and troubleshooting](../operations/observability-and-troubleshooting.md) - diagnosing derivations stuck in a status

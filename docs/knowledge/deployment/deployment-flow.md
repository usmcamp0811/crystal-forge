---
type: Workflow
title: "Deployment Flow"
description: "Describes how an agent receives and applies a desired target (heartbeat process, result types, configuration) and the automatic and manual deployment paths; open it when working on agent-side or user-triggered deployment."
tags:
  - crystal-forge
  - deployment
  - agent
  - heartbeat
  - workflow
implementation_status: implemented
generated:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-03T22:57:15-05:00
sources:
  - id: s1
    resource: "Crystal Forge repository file docs/derivation-status.md at commit 3b23d36f"
    title: "Crystal Forge Derivation Status Flow"
  - id: s2
    resource: "Crystal Forge repository file docs/specs/00-system-overview.md at commit 3b23d36f"
    title: "Crystal Forge - System Overview"
---

# Deployment Flow

> **Status:** Merged from the "Deployment Flow" section of the derivation status document and the "Deploying to a System" section of the system overview. Deployment result type names and configuration keys are verification candidates for `packages/default/crates/cf-agent/src/deployment/`.

## **Agent Heartbeat Process**
1. Agent sends system state to server
2. Server responds with `desired_target` (if any)
3. Agent compares desired target with current derivation path
4. If different: Execute deployment via `nixos-rebuild`
5. If same: Skip deployment (already current)
6. Report deployment result back to server

## **Deployment Result Types**
- `NoDeploymentNeeded`: No desired target set
- `AlreadyOnTarget`: Current and desired paths match
- `SuccessFromCache`: Deployment succeeded using binary cache
- `SuccessLocalBuild`: Deployment succeeded with local build
- `Failed`: Deployment failed with error details

## **Deployment Configuration**
- `dry_run_first`: Run dry-run before actual deployment
- `fallback_to_local_build`: Fall back to local build if cache fails
- `cache_url`: Binary cache URL for faster deployments
- `deployment_timeout_minutes`: Maximum deployment time

## 3. Deploying to a System

There are **two ways** to deploy:

### Automatic Deployment
1. New commit detected in tracked flake
2. Derivation automatically added to build queue
3. Builder builds and pushes to cache
4. Once in cache, deployment is triggered
5. Agent pulls from cache, activates config

### Manual Deployment (via UI)
1. User selects a system in UI
2. User selects a flake + branch + commit
3. UI shows what would change (diff)
4. User clicks "Deploy"

**How it works:**
- If the selected commit is **already in cache**: Agent pulls and activates instantly (no building needed)
- If the selected commit is **new/not built**: It goes to build queue first, then deployment happens after builder pushes to cache

**Key insight:** Because builders are always processing the queue and pushing to cache, most deployments are instant because the derivation is already cached.

**Key APIs:**
- `POST /systems/:id/deploy` - Trigger deployment
- `POST /agent/job/:id/complete` - Report result

## Related concepts

- [Commit to deploy sequence](../workflows/commit-eval-build-cache-deploy-sequence.md) - desired_target and agent heartbeat in sequence form
- [Derivation processing loops](../architecture/derivation-processing-loops.md) - the policy manager that sets desired_target
- [Authentication, authorization, and system registration](../security/authentication-and-authorization-overview.md) - who may deploy

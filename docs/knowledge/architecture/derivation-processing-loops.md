---
type: Architecture
title: "Derivation processing loops and integration points"
description: "Describes the evaluation, build, cache push, CVE scanning, and deployment policy loops (what each picks, interval, action) and how they integrate with system state, flakes, and security; open it when tuning or debugging a loop."
tags:
  - crystal-forge
  - architecture
  - loops
  - scheduling
  - cve
  - cache
implementation_status: partial
generated:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-03T22:57:15-05:00
sources:
  - id: origin
    resource: "Crystal Forge repository file docs/derivation-status.md at commit 3b23d36f"
    title: "Crystal Forge Derivation Status Flow"
  - id: code-1
    resource: "Crystal Forge repository file packages/default/crates/cf-server/src/server/mod.rs at commit 3b23d36f"
    title: Background task spawning and evaluation loop
  - id: code-2
    resource: "Crystal Forge repository file packages/default/crates/cf-server/src/queries/build_jobs.rs at commit 3b23d36f"
    title: Build job creation
  - id: code-3
    resource: "Crystal Forge repository file packages/default/crates/cf-server/src/handlers/api/builders.rs at commit 3b23d36f"
    title: Builder API handlers
  - id: code-4
    resource: "Crystal Forge repository file packages/default/crates/cf-builder/src/bin/builder.rs at commit 3b23d36f"
    title: Builder process
  - id: code-5
    resource: "Crystal Forge repository file packages/default/crates/cf-server/src/builder/cve_worker.rs at commit 3b23d36f"
    title: CVE scan loop
  - id: code-6
    resource: "Crystal Forge repository file packages/default/crates/cf-server/src/deployment/mod.rs at commit 3b23d36f"
    title: Deployment policy manager
---

# Derivation Processing Loops and Integration Points

> **Status:** partial. The loops below were corrected against the code (see Migration verification notes). The evaluation loop is commit-level and runs inside `cf-server`. Builds, cache pushes, and most CVE scans run in API-only `cf-builder` processes, not in server database loops. `get_derivations_ready_for_build()` exists (`packages/default/crates/cf-server/src/queries/derivations.rs`) but has no caller. `run_build_loop()` and `run_cache_push_workers()` exist in `packages/default/crates/cf-server/src/builder/` but the server binary does not start them. `update_auto_latest_policies()` runs in the deployment policy manager (`packages/default/crates/cf-server/src/deployment/mod.rs`).

## Processing Loops

### **Evaluation Loop (commit-level)**
- **Picks:** commits with `evaluation_status = 'pending'` that have a queued evaluation attempt whose `available_at` has passed, ordered by `eval_queue_position`, then commit time
- **Function:** `run_commit_evaluation_loop()` calling `process_pending_commits()` (`cf-server/src/server/mod.rs`)
- **Interval:** event-driven wakeup plus fallback tick, `flakes.commit_evaluation_interval` (default 60s)
- **Action:** `nix-eval-jobs` evaluates all `nixosConfigurations` of the commit inside `cf-server`, at most one commit at a time. The atomic finalize step writes the derivations directly as `dry-run-complete` (5) and queues build jobs.
- **Dependencies:** Discovers and inserts package dependencies (not verified; see notes)

### **Build Loop (builder API)**
- **Picks:** `build_jobs` rows in `queued` status. Rows are created for `dry-run-complete` derivations with the CF agent enabled and policy requirements met.
- **Function:** builders poll `next-job` (`cf-builder/src/bin/builder.rs`, `run_api_job_loop`)
- **Interval:** `builder.poll_interval` (default 5s) in each builder process. `build.poll_interval` (default 300s) belongs to the unused server-local `run_build_loop`.
- **Action:** `nix-store --realise` runs on the builder host (`cf-builder/src/derivations/build.rs`)
- **Features:** 
  - Systemd resource control (memory/CPU limits)
  - Streaming build progress with heartbeats
  - Real-time build target tracking

### **Cache Push (builder-side)**
- **Picks:** Derivations whose build succeeded on a builder
- **Function:** the builder pushes after the build and reports `cache_pushed` in the job completion request. The server then records a completed cache-push row (`complete_job`, `cf-server/src/handlers/api/builders.rs`). If a builder reports no push, the server queues a pending `cache_push_jobs` row for that derivation. No server process that claims such a row was found (`run_cache_push_workers()` is not started).
- **Interval:** not applicable to the builder-side push; `cache.poll_interval` (default 30s) belongs to the unused server-local worker
- **Action:** Push built derivations to binary cache
- **Backends:** S3, Attic, HTTP, Nix stores
- **Features:**
  - Parallel uploads
  - Retry logic with exponential backoff (`push_to_cache_with_retry`, `cf-builder/src/derivations/cache.rs`)
  - Filtering based on derivation names (not verified)

### **CVE Scanning Loop**
- **Picks:** Derivations with `build-complete` or `cache-pushed` status
- **Function:** `run_cve_scan_loop()` in the server (`cf-server/src/builder/cve_worker.rs`) for server-local vulnix runs and scan scheduling. Builders can also claim scans through `/api/v1/builders/:id/cve-scans/claim`.
- **Interval:** `vulnix.poll_interval` (default 60s)
- **Action:** Run vulnix security scanner
- **Output:** CVE database entries with severity levels

### **Deployment Policy Manager**
- **Picks:** Systems with `auto_latest` deployment policy
- **Function:** `update_auto_latest_policies()`
- **Interval:** `deployment.deployment_poll_interval`. The struct default is 60s; the NixOS module default is 15m.
- **Action:** Update system `desired_target` to latest successful derivation
- **Policies:** manual, auto_latest, pinned

## Integration Points

### **System State Tracking**
- Agents report current derivation path during heartbeats
- Server correlates system state with successful derivations
- Deployment timeline tracked via system state changes

### **Flake Integration**
- Commits trigger derivation creation
- Flake targets specify exact configuration paths
- Git references ensure reproducible builds

### **Security Integration**
- CVE scanning provides vulnerability assessment
- Binary cache signing ensures integrity
- Ed25519 authentication for agent communication

## Related concepts

- [Derivation status lifecycle](../concepts/derivation-status-lifecycle.md) - status IDs each loop picks
- [Event-driven queue architecture](event-driven-queues.md) - wakeups that complement these intervals
- [Cache push process](../caches/cache-push-process.md) - cache push loop details
- [Deployment flow](../deployment/deployment-flow.md) - agent-side handling of desired_target

## Migration verification notes

Scope: loop pickup conditions, functions, intervals, build and cache push placement, CVE loop, and deployment manager were compared with the code. Not checked: package dependency discovery, cache push filtering by derivation name, the "Integration Points" bullets beyond the Ed25519 builder and agent authentication.

- Claim: The evaluation loop picks `status_id = 3`, calls `process_pending_derivations()` every ~30s, and runs `nix build --dry-run`.
  Finding: No such function or dry-run command exists. Evaluation is commit-level (`run_commit_evaluation_loop`, `process_pending_commits`) using `nix-eval-jobs`. Finalize writes derivations as `dry-run-complete` (5). `DryRunPending` is used only when other code inserts new derivation rows (for example CVE helpers).
  Evidence: `cf-server/src/server/mod.rs`; `cf-server/src/models/evaluate_with_policies.rs` (`finalize_evaluation_attempt`); `rg -- '--dry-run' cf-server/src cf-builder/src` returns nothing.
  Case: documentation stale (corrected in place).
- Claim: The build loop picks `status_id IN (5, 7)` through `get_derivations_ready_for_build()` every ~60s.
  Finding: The function has no caller. Builds are `build_jobs` rows claimed by builders with `FOR UPDATE SKIP LOCKED`. `create_build_jobs_for_commit` selects `status_id = 5`, `cf_agent_enabled`, and `policy_requirements_met`.
  Evidence: `cf-server/src/queries/build_jobs.rs`; `cf-server/src/queries/builders.rs`; `cf-server/src/queries/derivations.rs`.
  Case: documentation stale (corrected in place).
- Claim: The server runs a cache push loop `process_cache_pushes()` every ~30s.
  Finding: `run_cache_push_workers()` and `run_build_loop()` are defined but not started by `bin/server.rs` or `spawn_background_tasks`. The builder pushes and reports `cache_pushed`.
  Evidence: `cf-server/src/bin/server.rs`; `cf-server/src/server/mod.rs` (`spawn_background_tasks`); `cf-server/src/handlers/api/builders.rs` (`complete_job`); `cf-builder/src/bin/builder.rs` (log line "Cache push performed builder-side").
  Case: documentation stale (corrected in place). The unclaimed fallback `cache_push_jobs` row is an observation, not confirmed as a defect.
- Claim: `scan_derivations()` runs every ~60s and picks `build-complete` or `cache-pushed` derivations.
  Finding: The function does not exist. `run_cve_scan_loop` is spawned from `spawn_background_tasks` with `vulnix.poll_interval`; its phases are stale recovery, prerequisite reconciliation, operator-queued scans, post-build scans, and periodic rescans.
  Evidence: `cf-server/src/builder/cve_worker.rs`; `cf-server/src/server/mod.rs`.
  Case: documentation stale (corrected in place).
- Claim: The deployment policy manager runs every ~15m.
  Finding: The interval is `deployment.deployment_poll_interval`; struct default 60s, NixOS module default 15m.
  Evidence: `cf-config/src/config/deployment.rs`; `modules/nixos/crystal-forge/default.nix`.
  Case: documentation stale (corrected in place).
- Claim: Policies are manual, auto_latest, pinned.
  Finding: Confirmed.
  Evidence: `cf-server/src/models/systems.rs` (`DeploymentPolicy`).
  Case: implemented.

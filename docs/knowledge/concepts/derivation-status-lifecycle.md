---
type: Concept
title: "Derivation status lifecycle"
description: "Defines the derivation status IDs and names, the build job statuses, the current commit-to-deploy lifecycle, terminal states, and retry rules; open it when reading or changing derivation, build job, or retry status handling."
tags:
  - crystal-forge
  - concept
  - derivation
  - status
  - retry
implementation_status: implemented
generated:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-04T21:00:00-05:00
sources:
  - id: origin
    resource: "Crystal Forge repository file docs/derivation-status.md at commit 3b23d36f"
    title: "Crystal Forge Derivation Status Flow (original document)"
  - id: code-1
    resource: "Crystal Forge repository file packages/default/crates/cf-server/migrations/0026_make_derivation_statuses.sql at commit 3b23d36f"
    title: Seeded derivation statuses 1 to 13
  - id: code-2
    resource: "Crystal Forge repository file packages/default/crates/cf-server/migrations/0052_create_cache_push_table.sql at commit 3b23d36f"
    title: Seeded derivation status 14
  - id: code-3
    resource: "Crystal Forge repository file packages/default/crates/cf-server/src/queries/builders.rs at commit 3b23d36f"
    title: Build job completion, failure, and automatic retry
  - id: code-4
    resource: "Crystal Forge repository file packages/default/crates/cf-server/src/queries/derivations.rs at commit 3b23d36f"
    title: EvaluationStatus enum and startup reset
  - id: code-5
    resource: "Crystal Forge repository file packages/default/crates/cf-server/migrations/0189_automatic_retry_policy.sql at commit 3b23d36f"
    title: Automatic retry policy singleton
---

# Derivation status lifecycle

Crystal Forge tracks one unit of work in four places. Each place answers a
different question.

| Record | Column | Question it answers |
| --- | --- | --- |
| `commits` | `evaluation_status` | Has the server evaluated this commit? |
| `derivations` | `status_id` | Did evaluation produce this derivation, and did a build finish? |
| `build_jobs` | `status` | Where is the build attempt in the builder queue? |
| `cache_push_jobs` | `status` | Is the output published to a binary cache? |

> **Scope:** This document describes the code at commit `3b23d36f`. The
> `derivations.status_id` column keeps the 14 seeded statuses, but the current
> server writes only a subset of them. The tables below mark which statuses are
> written today.

## Current lifecycle

The server evaluates a commit as a whole with `nix-eval-jobs`. API-only builders
build the result and publish it to the binary cache. Agents deploy from the
cache. The database never receives writes from a builder directly.

```mermaid
sequenceDiagram
    participant G as Git source
    participant S as Server
    participant DB as PostgreSQL
    participant B as API-only builder
    participant C as Binary cache
    participant A as Agent

    G->>S: Webhook or sync finds a new commit
    S->>DB: Insert commit, evaluation_status pending
    S->>DB: Mark commit in_progress, one commit at a time
    S->>S: nix-eval-jobs evaluates all systems
    alt Evaluation of a system succeeds
        S->>DB: Insert or update derivation, status dry-run-complete 5
        S->>DB: Insert build_jobs row, status queued
    else Evaluation of a system fails
        S->>DB: Record derivation, status dry-run-failed 6
    end
    S->>DB: Mark commit complete or failed

    loop Builder polls every builder.poll_interval
        B->>S: Claim next job, signed request
        S->>DB: build_jobs queued to building
        S-->>B: Job payload
    end
    B->>B: nix build
    B->>C: Sign and push the output
    alt Build and push succeed
        B->>S: Complete job with store path and cache reference
        S->>DB: build_jobs building to success
        S->>DB: Derivation to build-complete 10, store_path set
        S->>C: nix path-info probe
        S->>DB: cache_push_jobs row completed
    else Build or push fails
        B->>S: Fail job with phase and failure class
        S->>DB: build_jobs building to failed
        opt Retry budget remains and the failure class is eligible
            S->>DB: Insert child build_jobs row, queued, available_at in the future
        end
    end

    A->>S: Heartbeat
    S-->>A: desired_target when policy gates allow it
    A->>C: nix copy the store path
    A->>A: switch-to-configuration through systemd-run
    A->>S: Report deployment-started or deployment-failed
```

Key properties:

- Evaluation is commit-level. The server does not run a per-derivation
  `nix build --dry-run` loop, and `dry-run-complete` means that
  `nix-eval-jobs` produced the derivation path.
- The server starts neither a build loop nor a cache push worker. A builder
  claims jobs through the API and reports results through the API. See
  [Builder architecture](../builders/builder-architecture-and-job-scheduling.md).
- Build progress lives on `build_jobs.status`. The claim and the failure
  transition do not change `derivations.status_id`.
- A completed build changes the derivation to `build-complete` in the same
  transaction that marks the job `success`.
- Deployment starts from a heartbeat response. See
  [Deployment flow](../deployment/deployment-flow.md).

## Derivation statuses

The `derivation_statuses` table holds these rows. IDs 1 to 13 come from migration
`0026`. ID 14 comes from migration `0052`. The `Name` column shows the database
spelling.

| ID | Name | Terminal in seed | Written by current code |
| --: | --- | :-: | --- |
| 1 | `pending` | No | No |
| 2 | `queued` | No | No |
| 3 | `dry-run-pending` | No | Only as an upsert starting value and by startup reset |
| 4 | `dry-run-inprogress` | No | No |
| 5 | `dry-run-complete` | No | Yes. Evaluation inserts rows directly in this status. |
| 6 | `dry-run-failed` | Yes | Yes. Evaluation failure records it. |
| 7 | `build-pending` | No | Only by startup reset |
| 8 | `build-inprogress` | No | Only by the server-side build worker code, which the server does not start |
| 9 | `in-progress` | No | No |
| 10 | `build-complete` | No | Yes. Job completion writes it with `store_path`. |
| 11 | `complete` | Yes | No |
| 12 | `build-failed` | Yes | Only by startup reset. `mark_derivation_failed` has no production caller. |
| 13 | `failed` | Yes | No |
| 14 | `cache-pushed` | Yes | Only by dormant server cache worker code |

The Rust `EvaluationStatus` enum covers IDs 3, 4, 5, 6, 7, 8, 10, and 12. The
dashboard queries (`queries/dashboard.rs`) still read IDs 10, 11, 12, and 14
when they classify a derivation as built or cached.

**The seed marks `dry-run-failed` and `build-failed` as terminal without a retry
exception.** The retry rules below are enforced by queries, not by these flags.

## Build job statuses

`build_jobs.status` accepts these values. The check constraint comes from
migration `0083` and migration `0103`.

| Status | Meaning | Next |
| --- | --- | --- |
| `queued` | Waiting for a builder. A job with a future `available_at` cannot be claimed yet. | `building`, `cancelled` |
| `building` | A builder owns the job under a session lease. | `success`, `failed`, `cancelling` |
| `cancelling` | An operator requested a stop. The builder has not stopped yet. | `cancelled` |
| `cancelled` | Stopped by an operator. Terminal. | None |
| `success` | The builder completed the job. Terminal. | None |
| `failed` | The attempt failed. Terminal for this attempt. | A new child job, if retry applies |

## Retry rules

### Automatic build retry

When a builder fails a job, one transaction marks that attempt `failed` and
decides on a retry. The decision reads the singleton `automatic_retry_policy`
row (migration `0189`):

| Column | Default | Allowed values | Effect |
| --- | --- | --- | --- |
| `max_build_retries` | 2 | 0 to 5 | Number of child attempts after the first attempt |
| `max_evaluation_retries` | 1 | 0 to 5 | Same limit for evaluation retries |
| `backoff_seconds` | 30 | 0, 10, 30, 60, 120, 300 | Delay before the child job becomes claimable |
| `transient_only` | true | true or false | When true, only failure classes that can recover cause a retry |

A retry inserts a **new** `build_jobs` row with `parent_job_id`, `root_job_id`,
`attempt_number + 1`, and a future `available_at`. It does not reopen the failed
row. A unique key on `automatic_retry_source_id` prevents duplicate children.
When no retry applies, the server also opens an attention item for the failed
job.

### Startup reset

At server start, `reset_non_terminal_derivations` runs once. It changes
derivations as follows:

- A derivation with `attempt_count >= 5` becomes `dry-run-failed` (6) when it
  has no `derivation_path`, or `build-failed` (12) when it has one.
- A derivation with `attempt_count < 5` and a status other than
  `dry-run-complete` (5) and `build-complete` (10) returns to `dry-run-pending`
  (3) without a path, or `build-pending` (7) with a path.

The API builder path does not increment `attempt_count`, so this reset mostly
affects rows that the older server-side build path touched. The evaluation loop
runs its own startup recovery. See
[Evaluation and build queue pipeline](../workflows/evaluation-and-build-queue-pipeline.md).

### Manual intervention

- Requeue or cancel a build job through the build job API.
- Re-evaluate a commit to produce a fresh derivation row.
- Change the retry policy with `PUT /api/v1/admin/automatic-retry-policy`.

## Terminal states

- **Derivation `build-complete` (10):** The artifact exists. It is deployable
  only when a `completed` `cache_push_jobs` row matches `derivations.store_path`.
  See [Store path flow](../workflows/store-path-flow.md).
- **Derivation `dry-run-failed` (6):** Evaluation failed. A new evaluation of the
  commit is required.
- **Build job `failed` with no child job:** The retry budget is spent or the
  failure class is not eligible. An operator may requeue the job.
- **Build job `success` and `cancelled`:** Final. No transition leaves them.

## Related concepts

- [Derivation processing loops](../architecture/derivation-processing-loops.md) - the loops that run on the server
- [Cache push process](../caches/cache-push-process.md) - builder-side publication and the server probe
- [Evaluation and build queue pipeline](../workflows/evaluation-and-build-queue-pipeline.md) - the two-stage pipeline and its queue APIs
- [Deployment flow](../deployment/deployment-flow.md) - how an agent applies a target
- [Observability and troubleshooting](../operations/observability-and-troubleshooting.md) - diagnosing work stuck in a status

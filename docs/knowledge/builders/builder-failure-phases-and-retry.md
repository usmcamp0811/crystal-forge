---
type: Design Specification
title: "Builder failure phases and retry strategy"
description: "Lists the pre-build failure phases a builder reports (source_fetch through build), which of them retry or fail permanently, and the priority-weighting retry and max-retries rules for build jobs."
tags:
  - crystal-forge
  - builder
  - retry
  - failure-phase
implementation_status: implemented
generated:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-03T22:57:42-05:00
sources:
  - id: origin
    resource: "Crystal Forge repository file docs/builder-security-architecture.md at commit 3b23d36f"
    title: "Crystal Forge Builder Security Architecture"
  - id: origin-mba
    resource: "Crystal Forge repository file docs/multi-builder-api.md at commit 3b23d36f"
    title: "Multi-Builder API Documentation"
---

# Builder failure phases and retry strategy

## 10. Pre-Build Failure Phases

When a build cannot proceed safely, the builder reports a specific failure phase rather than leaving the job in `building` state indefinitely.

| Phase | Trigger | Job State |
|---|---|---|
| `source_fetch` | Artifact download or transient local I/O failed | `failed` or retry |
| `source_identity_mismatch` | Artifact digest, format, extraction safety, lock digest, store name, or NAR identity differs | `failed` (no build) |
| `source_input_availability` | Required source inputs not available | `failed` or retry |
| `evaluator_incompatible` | Contract version, Nix version, evaluator system, purity, lock mutation, IFD, or source schema differs | Released to `queued` for another compatible builder; retry budget unchanged |
| `evaluation` | `nix eval .drvPath` failed on builder (timeout, eval error) | `failed` or retry |
| `derivation_mismatch` | Builder-evaluated `.drvPath` ≠ server-expected `.drvPath` | `failed` (no retry — policy violation) |
| `path_materialization` | `.drv` not available locally after delta manifest/archive or full archive download | `failed` or retry |
| `delta_unsupported` (info only) | Delta endpoint returned 404/405 — transparent fallback to full archive | (not a failure) |
| `build` | `nix-store --realise` failed | `failed` or retry |

`derivation_mismatch` is treated as a hard failure with no automatic retry because it indicates the build plan has diverged from the server-evaluated state, which is a security-relevant event that warrants human review.

## Retry Strategy

### Priority Weighting

Jobs have a `priority_weight` (default 1.0, higher = higher priority).

**On Retry**:
- Priority reduced by 5%: `new_priority = old_priority * 0.95`
- Ensures newer commits don't wait behind long retry queues
- Strategy: fail X → build Y → retry X → build Z → retry X

**Example**:
```
Job A: priority 1.0  (attempt 1)
Job A: priority 0.95 (attempt 2, after retry)
Job B: priority 1.0  (attempt 1, new commit)
Job A: priority 0.90 (attempt 3, after retry)
```

Queue order: B (1.0), A (0.95), A (0.90)

### Max Retries

Configurable per job (default: 3).

After exceeding max retries:
- Status → "failed"
- Job removed from queue
- Marked permanently failed

## Related concepts

* [Verified-source evaluator contract (source_re_evaluate_verified)](verified-source-evaluator-contract.md) - Specifies the verified-source flow where the builder re-evaluates a canonical source archive and compares its .drvPath to the server value, including the evaluator fingerprint, next-job 409 reasons, and rolling-upgrade behavior.
* [Multi-Builder API architecture, scheduling, and environment assignment](builder-architecture-and-job-scheduling.md) - Describes the multi-builder architecture, environment assignment (wildcard and specific builders), heartbeat and offline detection, query performance, the migration from direct database access, and future enhancements.
* [Builder API database schema](../data-model/builder-api-database-schema.md) - Lists the builders, builder_environment_assignments, build_jobs, and builder_metrics tables with their SQL definitions, job states, and retry columns used by the multi-builder API.
* [Builder API: job lifecycle endpoints](../api/builder-job-lifecycle-api.md) - Documents the builder-signed endpoints for heartbeat, next-job polling (including 409 evaluator conflicts), derivation manifest and delta/full derivation archives, job completion, failure, and log append.

---
type: Data Model
title: "Flake Recent Commits View (`view_flake_recent_commits`)"
description: "Describes view_flake_recent_commits, the SQL view of the last three commits per flake with an attempt status derived from the raw commits.attempt_count counter (ok, retries, failed/stuck at 5 or more); its only consumers are the legacy dashboard JSON and the database test suite."
tags:
  - crystal-forge
  - view
  - flakes
  - commits
implementation_status: implemented
sources:
  - id: origin
    resource: "Crystal Forge repository file docs/views/view_flake_recent_commits.md at commit 3b23d36f"
    title: "Flake Recent Commits View (`view_flake_recent_commits`)"
---
# Flake Recent Commits View (`view_flake_recent_commits`)

## Overview

The `view_flake_recent_commits` provides a **per-flake snapshot of the most recent commits** (default: last 3). It highlights commit attempt activity while suppressing “retries” if the commit has progressed into the derivation stage.

**Consumers at revision `3b23d36f`:** the legacy Grafana dashboard definition (`packages/dashboards/crystal-forge-dashboard.json`) and the database tests (`packages/cf-test-suite/cf_test/tests/database/test_view_flake_recent_commits.py`). The Dioxus UI and the server API do not read this view. It was defined once, by migration `0051_create_view_flake_recent_commits.sql`.

**Attempt semantics:** the view reads the raw `commits.attempt_count` column and uses the fixed threshold 5. Evaluation retries are governed separately: the loop counts `evaluation_attempt_count` against the `automatic_retry_policy` limit (default 1 evaluation retry). The `failed/stuck threshold` label therefore does not mean that automatic retries are exhausted. See [Wakeups and polling](../../architecture/event-driven-queues.md#retry-delay).

## Key Behavior

- **Per-flake limit**: Shows only the last 3 commits per flake.
- **Attempt threshold**: Flags commits at or above 5 attempts as failed/stuck.
- **Retries logic**:

  - If `attempt_count > 0` and no derivations exist → **`retries`**
  - If derivations exist → treated as **`ok`** even with retries.

- **Time metrics**: Provides both an interval and numeric minutes since commit.

## Important Fields

| Field                  | Description                                                          |
| ---------------------- | -------------------------------------------------------------------- |
| `flake`                | Flake/repository name                                                |
| `commit`               | 12-character short Git commit hash                                   |
| `commit_timestamp`     | When the commit was created                                          |
| `attempt_count`        | Number of commit attempts (raw counter)                              |
| `attempt_status`       | Status: `ok`, `retries`, or `⚠︎ failed/stuck threshold`             |
| `minutes_since_commit` | Integer minutes since commit was created                             |
| `age_interval`         | Interval value (`NOW() - commit_timestamp`) for precise time display |

## Data Ordering

Results are ordered by:

1. **Flake name**
2. **Commit timestamp DESC** (newest first)

This makes it easy to group and scan per-flake activity.

## Primary Use Cases

- **Commit Health Monitoring**: Quickly identify commits stuck in retry loops or at the failed/stuck threshold.
- **Pipeline Progress Insight**: Verify that retries are only flagged when commits haven’t yet advanced to derivations.

## Example Queries

```sql
-- Show latest 3 commits per flake with attempt status
SELECT *
FROM view_flake_recent_commits
ORDER BY flake, commit_timestamp DESC;

-- Filter to a single flake
SELECT *
FROM view_flake_recent_commits
WHERE flake = 'crystal-forge'
ORDER BY commit_timestamp DESC;

-- Show only commits at or above threshold
SELECT *
FROM view_flake_recent_commits
WHERE attempt_status = '⚠︎ failed/stuck threshold'
ORDER BY commit_timestamp DESC;
```

## Integration Notes

This view complements:

- **`view_commit_build_status`** → deeper build/derivation details
- **`view_commit_deployment_timeline`** → temporal deployment tracking
- **`view_commit_nixos_table`** → commit-centric NixOS derivation breakdown

Together, these views cover the full lifecycle: **commit creation → build/eval → deployment → recent status snapshots**.

## Related concepts

- [Commit Build Status View](view-commit-build-status.md)
- [Commit Deployment Timeline View](view-commit-deployment-timeline.md)
- [Commit NixOS Table View](view-commit-nixos-table.md)

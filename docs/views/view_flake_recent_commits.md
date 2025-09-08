# Flake Recent Commits View (`view_flake_recent_commits`)

## Overview

The `view_flake_recent_commits` provides a **per-flake snapshot of the most recent commits** (default: last 3). It highlights commit attempt activity while suppressing “retries” if the commit has progressed into the derivation stage. This view is designed for **Grafana dashboards** as a compact table for quick health/status checks.

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
| `minutes_since_commit` | Integer minutes since commit was created (for easy Grafana display)  |
| `age_interval`         | Interval value (`NOW() - commit_timestamp`) for precise time display |

## Data Ordering

Results are ordered by:

1. **Flake name**
2. **Commit timestamp DESC** (newest first)

This makes it easy to group and scan per-flake activity.

## Primary Use Cases

- **Commit Health Monitoring**: Quickly identify commits stuck in retry loops or at the failed/stuck threshold.
- **Pipeline Progress Insight**: Verify that retries are only flagged when commits haven’t yet advanced to derivations.
- **Grafana Dashboards**: Compact per-flake table panel showing latest commit health and timing.

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

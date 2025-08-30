# NixOS Commit Table View (`view_commit_nixos_table`)

## Overview

`view_commit_nixos_table` is a **compact, commit-centric table** for dashboards (half-width panels). It lists **NixOS** derivations for a given commit and includes small **commit-level aggregates** suitable for a progress/gauge visual.

> Packages are **excluded**. Only `derivation_type = 'nixos'` rows are shown.

## Columns

| Column              | Type          | Notes                                         |
| ------------------- | ------------- | --------------------------------------------- |
| `commit_id`         | `bigint`      | Commit PK                                     |
| `git_commit_hash`   | `text`        | Full hash                                     |
| `short_hash`        | `text`        | 8-char hash                                   |
| `commit_timestamp`  | `timestamptz` | Commit time                                   |
| `flake_name`        | `text`        | Flake/repo name                               |
| `derivation_name`   | `text`        | NixOS derivation name                         |
| `derivation_status` | `text`        | Status at row level                           |
| `status_order`      | `int`         | Sort hint from `derivation_statuses`          |
| `total`             | `int`         | Count of NixOS derivations for the commit     |
| `successful`        | `int`         | Successful NixOS derivations                  |
| `failed`            | `int`         | Failed (terminal & not success)               |
| `in_progress`       | `int`         | Non-terminal                                  |
| `progress_pct`      | `numeric`     | `ROUND(100 * successful / total, 1)` (0 if 0) |

## Typical Use

- **Filter by commit** (dropdown → hash), show per-derivation rows + a small **progress** (use `progress_pct`).
- Sort table by `status_order`, then `derivation_name` for stable grouping.

## Example Queries

### All NixOS rows for a commit (panel table)

```sql
SELECT
  commit_timestamp,
  flake_name,
  short_hash,
  derivation_name,
  derivation_status,
  progress_pct
FROM public.view_commit_nixos_table
WHERE git_commit_hash = $commit_hash  -- Grafana/variable
ORDER BY status_order, derivation_name;
```

### One row per commit (header/progress panel)

```sql
SELECT DISTINCT
  commit_timestamp,
  flake_name,
  short_hash,
  total, successful, failed, in_progress,
  progress_pct
FROM public.view_commit_nixos_table
WHERE git_commit_hash = $commit_hash;
```

### Latest N commits for a flake (for a selector)

```sql
SELECT DISTINCT ON (git_commit_hash)
  commit_timestamp, flake_name, git_commit_hash, short_hash
FROM public.view_commit_nixos_table
WHERE flake_name = $flake
ORDER BY git_commit_hash, commit_timestamp DESC
LIMIT 20;
```

## Panel Tips (Grafana)

- **Progress**: Use `progress_pct` as a **gauge** or **bar** (field override 0–100).
- **Row coloring**: Map `derivation_status` to thresholds/colors.
- **Compact**: Hide non-essential columns for half-width.

## Related

- Build status across all derivation types: `view_commit_build_status`
- Deploy/config timeline: `view_config_timeline`
- Per-system deployment state: `view_system_deployment_status`

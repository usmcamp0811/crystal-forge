---
type: Data Model
title: "NixOS Commit Table View (`view_commit_nixos_table`)"
description: "Describes view_commit_nixos_table, the SQL view that lists the NixOS derivations of a commit with commit-level progress aggregates; its only consumer is the database test suite."
tags:
  - crystal-forge
  - view
  - commits
  - nixos
implementation_status: implemented
sources:
  - id: origin
    resource: "Crystal Forge repository file docs/views/view_commit_nixos_table.md at commit 3b23d36f"
    title: "NixOS Commit Table View (`view_commit_nixos_table`)"
---
# NixOS Commit Table View (`view_commit_nixos_table`)

## Overview

`view_commit_nixos_table` is a **compact, commit-centric table**. It lists **NixOS** derivations for a given commit and includes small **commit-level aggregates** (`total`, `successful`, `failed`, `in_progress`, `progress_pct`).

**Consumers at revision `3b23d36f`:** only the database test (`packages/cf-test-suite/cf_test/tests/database/test_view_commit_nixos_table.py`). No application code, UI, API, or dashboard definition reads this view. It was defined once, by migration `0047_revamp_views.sql`.

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

- Filter by commit hash and read the per-derivation rows together with `progress_pct`.
- Sort by `status_order`, then `derivation_name`, for stable grouping.

## Example Queries

### All NixOS rows for a commit

```sql
SELECT
  commit_timestamp,
  flake_name,
  short_hash,
  derivation_name,
  derivation_status,
  progress_pct
FROM public.view_commit_nixos_table
WHERE git_commit_hash = $commit_hash
ORDER BY status_order, derivation_name;
```

### One row per commit

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

### Latest N commits for a flake

```sql
SELECT DISTINCT ON (git_commit_hash)
  commit_timestamp, flake_name, git_commit_hash, short_hash
FROM public.view_commit_nixos_table
WHERE flake_name = $flake
ORDER BY git_commit_hash, commit_timestamp DESC
LIMIT 20;
```

## Related

- Build status across all derivation types: `view_commit_build_status`
- Deploy/config timeline: `view_config_timeline`
- Per-system deployment state: `view_system_deployment_status`

## Related concepts

- [Commit Build Status View](view-commit-build-status.md)
- [Config Timeline View](view-config-timeline.md)
- [System Deployment Status View](view-system-deployment-status.md)

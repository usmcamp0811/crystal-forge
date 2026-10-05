---
type: Data Model
title: "Buildable Derivations View (`view_buildable_derivations`)"
description: "Describes the current view_buildable_derivations definition (migration 0070): unreserved NixOS-system derivations ordered newest commit first, used only by the legacy build-reservation worker that the server no longer starts; the live build queue is build_jobs."
tags:
  - crystal-forge
  - view
  - build-queue
  - workers
  - legacy
implementation_status: historical
status: deprecated
generated:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-04T17:10:00-05:00
sources:
  - id: origin
    resource: "Crystal Forge repository file docs/views/view_buildable_derivations.md at commit 3b23d36f"
    title: "Buildable Derivations View (`view_buildable_derivations`) (original document)"
  - id: code-1
    resource: "Crystal Forge repository file packages/default/crates/cf-server/migrations/0070_update_view_nixos_pipeline_latest_with_deploy.sql at commit 3b23d36f"
    title: Current view definition
  - id: code-2
    resource: "Crystal Forge repository file packages/default/crates/cf-server/src/queries/build_reservations.rs at commit 3b23d36f"
    title: Legacy reservation claim
  - id: code-3
    resource: "Crystal Forge repository file packages/default/crates/cf-server/src/queries/builders.rs at commit 3b23d36f"
    title: Live build_jobs claim
---

# Buildable Derivations View (`view_buildable_derivations`)

## Status

The view exists in the schema. It is the claim source of the legacy
**build-reservation** worker, which the server does not start:

- `claim_next_derivation` in `cf-server/src/queries/build_reservations.rs` reads
  the view and inserts a `build_reservations` row. Only the legacy
  `builder/worker.rs` path calls it, and `run_build_loop` has no caller in the
  server startup code.
- `get_derivations_ready_for_build` in `queries/derivations.rs` joins the view
  and also has no caller.
- The Dioxus UI and the server API do not read the view. The database tests
  (`packages/cf-test-suite/cf_test/tests/database/test_view_buildable_derivations.py`,
  `tests/builder/test_reservation_build_queue.py`) do.

The live build queue is `build_jobs`, claimed by API-only builders. Its
eligibility and ordering are different. See
[Wakeups and polling](../../architecture/event-driven-queues.md#claim-eligibility-and-ordering).

## Current definition

Migration `0070_update_view_nixos_pipeline_latest_with_deploy.sql` drops and
recreates the view. It is the last definition. Earlier definitions (migrations
`0057` and `0058`) are superseded and are not described here.

The view lists **NixOS system derivations only**. Package derivations never
appear.

### Columns

| Column | Description |
| --- | --- |
| `id` | Derivation id |
| `derivation_name` | Derivation name |
| `derivation_type` | Always `nixos` |
| `derivation_path` | Store derivation path (never null in this view) |
| `status_id` | `5` (`dry-run-complete`) or `7` (`build-pending`) |
| `nixos_id` | Same as `id` |
| `nixos_commit_ts` | Commit timestamp of the derivation's commit |
| `active_workers` | Count of reservations on the derivation. Always `0` here, because reserved rows are excluded. |
| `queue_position` | `ROW_NUMBER()` over `nixos_commit_ts DESC, id ASC` |

### Row filter

A derivation appears when all of these hold:

- `derivation_type = 'nixos'`;
- `status_id IN (5, 7)`;
- `derivation_path IS NOT NULL`;
- `attempt_count <= 5`;
- no `build_reservations` row references it.

The view does not filter on `cf_agent_enabled` or `policy_requirements_met`.
The live `build_jobs` claim requires both.

### Ordering

Newest commit first (`nixos_commit_ts DESC`), then lower `id`. `queue_position`
numbers rows in that order.

## Legacy claim sequence

`claim_next_derivation` runs in one transaction:

1. Select the first row of the view ordered by `queue_position`.
2. Insert a `build_reservations` row with `ON CONFLICT (derivation_id) DO NOTHING`.
   If another worker reserved it, roll back and return no work.
3. Set `status_id` to `BuildInProgress`, set `started_at`, and increment
   `attempt_count`, only when the status is `DryRunComplete` or `BuildPending`.
   If no row changes, delete the reservation, roll back, and return no work.
4. Load and return the full derivation.

## Example queries

```sql
SELECT id, derivation_name, status_id, nixos_commit_ts, queue_position
FROM view_buildable_derivations
ORDER BY queue_position
LIMIT 10;
```

```sql
SELECT status_id, COUNT(*) AS systems
FROM view_buildable_derivations
GROUP BY status_id;
```

## Relationship to other views

[`view_nixos_derivation_build_queue`](view-nixos-derivation-build-queue.md) is
the earlier queue view (migration `0056_build_queue.sql`). No migration drops it,
and it has its own database test. [`view_build_queue_status`](view-build-queue-status.md)
reports system-level progress for the same legacy queue. It reads base tables,
not this view, so recreating this view did not affect it.

## Related tables

- **`build_reservations`** - legacy worker assignments
- **`derivations`** - core derivation table
- **`commits`** - commit timestamps for ordering

## Related concepts

- [Build Queue Status View](view-build-queue-status.md)
- [NixOS Derivation Build Queue View](view-nixos-derivation-build-queue.md)

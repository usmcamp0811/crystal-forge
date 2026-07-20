-- Persist ordered branch commit visibility for database-only Flakes read path.
--
-- Before this migration, GET /api/v1/flakes/timelines performed a git clone +
-- git log for every flake to determine remote branch order (for filtering
-- force-pushed commits) and to hydrate git metadata. This made timeline reads
-- sequentially dependent on git/network latency and unreachable when git is
-- unavailable.
--
-- This migration adds:
--   1. A read model table for ordered branch visibility
--   2. A per-flake readiness marker for safe rollout
--   3. A covering index for deterministic recent-commit ordering
--
-- See TASK-397 for full design.

-- 1. Branch-commit snapshot table
--
-- Stores the ordered list of commits currently visible on a flake's tracked
-- remote branch. Populated atomically by successful synchronization and read
-- by GET handlers. Position 0 is the current remote branch HEAD.
--
-- The snapshot is replaced atomically in a single transaction: readers see
-- either the previous complete snapshot or the new complete snapshot, never
-- a partial result.

CREATE TABLE IF NOT EXISTS public.flake_branch_commit_snapshot (
    flake_id    integer     NOT NULL REFERENCES public.flakes(id) ON DELETE CASCADE,
    commit_id   integer     NOT NULL REFERENCES public.commits(id) ON DELETE CASCADE,
    position    integer     NOT NULL CHECK (position >= 0),
    observed_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (flake_id, commit_id),
    UNIQUE (flake_id, position)
);

COMMENT ON TABLE public.flake_branch_commit_snapshot IS
    'Ordered snapshot of commits currently visible on a flake tracked branch. '
    'Populated atomically by successful sync. Position 0 = branch HEAD.';

COMMENT ON COLUMN public.flake_branch_commit_snapshot.position IS
    'Commit order on the tracked branch. 0 = HEAD, incremented by 1 for each '
    'ancestor. Unique per flake for deterministic ordering.';

COMMENT ON COLUMN public.flake_branch_commit_snapshot.observed_at IS
    'Server timestamp when this snapshot was created during sync.';


-- 2. Snapshot-ready marker
--
-- Distinguishes "no snapshot has ever been populated" from "snapshot is
-- currently empty". The server falls back to deterministic timestamp-based
-- ordering until the first successful post-migration sync populates the
-- snapshot.

ALTER TABLE public.flakes
    ADD COLUMN IF NOT EXISTS snapshot_ready_at timestamptz;

COMMENT ON COLUMN public.flakes.snapshot_ready_at IS
    'Set to now() on first successful snapshot population. NULL means this '
    'flake has never had a branch snapshot — readers fall back to '
    'timestamp-based ordering until first sync.';


-- 3. Ordering index for recent-commit lookups
--
-- Both the snapshot-based and fallback timeline queries filter/order by
-- (flake_id, commit_timestamp DESC). Without this index, those queries
-- perform a sequential scan + sort per flake. The id DESC tiebreaker
-- ensures deterministic order when two commits have identical timestamps.

CREATE INDEX IF NOT EXISTS idx_commits_flake_id_timestamp_desc_id_desc
    ON public.commits (flake_id, commit_timestamp DESC, id DESC);

COMMENT ON INDEX public.idx_commits_flake_id_timestamp_desc_id_desc IS
    'Covering index for timeline and registry queries ordering commits by '
    'timestamp descending per flake. Added in TASK-397.';

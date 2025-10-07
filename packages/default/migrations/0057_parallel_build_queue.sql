-- Migration 0057: Add build reservations table and update build queue view
-- This enables system-aware parallel building with worker coordination
-- Step 1: Create build_reservations table
CREATE TABLE IF NOT EXISTS build_reservations (
    id serial PRIMARY KEY,
    worker_id text NOT NULL,
    derivation_id integer NOT NULL REFERENCES derivations (id) ON DELETE CASCADE,
    nixos_derivation_id integer REFERENCES derivations (id) ON DELETE CASCADE,
    reserved_at timestamptz NOT NULL DEFAULT NOW(),
    heartbeat_at timestamptz NOT NULL DEFAULT NOW(),
    CONSTRAINT build_reservations_derivation_unique UNIQUE (derivation_id)
);

-- Step 2: Add indexes for performance
CREATE INDEX idx_build_reservations_worker ON build_reservations (worker_id);

CREATE INDEX idx_build_reservations_nixos ON build_reservations (nixos_derivation_id);

CREATE INDEX idx_build_reservations_heartbeat ON build_reservations (heartbeat_at);

-- Step 3: Add comments for documentation
COMMENT ON TABLE build_reservations IS 'Tracks which worker is building which derivation. Reservations are temporary and deleted when build completes or fails.';

COMMENT ON COLUMN build_reservations.worker_id IS 'Unique identifier for the worker task (e.g., "builder-hostname-worker-0")';

COMMENT ON COLUMN build_reservations.derivation_id IS 'The derivation this worker is currently building';

COMMENT ON COLUMN build_reservations.nixos_derivation_id IS 'The parent NixOS system this package belongs to (NULL for system builds)';

COMMENT ON COLUMN build_reservations.reserved_at IS 'When this work was claimed by the worker';

COMMENT ON COLUMN build_reservations.heartbeat_at IS 'Last heartbeat timestamp - used to detect crashed/hung workers';

-- Step 4: Create the new buildable derivations view
-- This view respects the wait_for_cache_push config setting
CREATE OR REPLACE VIEW view_buildable_derivations AS
WITH system_progress AS (
    -- For each NixOS system, calculate total and completed packages
    SELECT
        r.nixos_id,
        r.nixos_commit_ts,
        COUNT(DISTINCT p.id) AS total_packages,
        COUNT(DISTINCT p.id) FILTER (WHERE p.status_id = 6 -- BuildComplete
) AS completed_packages,
        COUNT(DISTINCT p.id) FILTER (WHERE p.status_id = 14 -- CachePushed
) AS cached_packages,
        COUNT(DISTINCT br.id) AS active_workers
    FROM (
        SELECT
            d.id AS nixos_id,
            c.commit_timestamp AS nixos_commit_ts
        FROM
            derivations d
            JOIN commits c ON c.id = d.commit_id
        WHERE
            d.derivation_type = 'nixos'
            AND d.status_id IN (5, 12) -- DryRunComplete, Scheduled
) r
        LEFT JOIN derivation_dependencies dd ON dd.derivation_id = r.nixos_id
        LEFT JOIN derivations p ON p.id = dd.depends_on_id
            AND p.derivation_type = 'package'
        LEFT JOIN build_reservations br ON br.nixos_derivation_id = r.nixos_id
    GROUP BY
        r.nixos_id,
        r.nixos_commit_ts
),
package_candidates AS (
    -- Packages that can be built (not reserved, not complete)
    SELECT
        p.id,
        p.derivation_name,
        p.derivation_type,
        p.derivation_path,
        p.pname,
        p.version,
        p.status_id,
        sp.nixos_id,
        sp.nixos_commit_ts,
        sp.total_packages,
        sp.completed_packages,
        sp.active_workers,
        'package' AS build_type,
        ROW_NUMBER() OVER (ORDER BY sp.nixos_commit_ts DESC,
            sp.total_packages ASC, -- Smaller systems first within same commit
            sp.nixos_id,
            p.pname NULLS LAST) AS queue_position
    FROM
        system_progress sp
        JOIN derivation_dependencies dd ON dd.derivation_id = sp.nixos_id
        JOIN derivations p ON p.id = dd.depends_on_id
        LEFT JOIN build_reservations br ON br.derivation_id = p.id
    WHERE
        p.derivation_type = 'package'
        AND p.status_id IN (5, 12) -- DryRunComplete, Scheduled
        AND p.attempt_count <= 5
        AND br.id IS NULL -- Not already reserved
),
system_candidates AS (
    -- NixOS systems that can be built (all packages complete)
    SELECT
        d.id,
        d.derivation_name,
        d.derivation_type,
        d.derivation_path,
        NULL::text AS pname,
        NULL::text AS version,
        d.status_id,
        sp.nixos_id,
        sp.nixos_commit_ts,
        sp.total_packages,
        sp.completed_packages,
        sp.active_workers,
        'system' AS build_type,
        ROW_NUMBER() OVER (ORDER BY sp.nixos_commit_ts DESC,
            sp.total_packages ASC,
            sp.nixos_id) AS queue_position
    FROM
        system_progress sp
        JOIN derivations d ON d.id = sp.nixos_id
        LEFT JOIN build_reservations br ON br.derivation_id = d.id
    WHERE
        d.derivation_type = 'nixos'
        AND d.status_id IN (5, 12)
        AND d.attempt_count <= 5
        AND br.id IS NULL
        AND sp.total_packages = sp.completed_packages -- All deps complete
)
SELECT
    *
FROM
    package_candidates
UNION ALL
SELECT
    *
FROM
    system_candidates
ORDER BY
    queue_position;

COMMENT ON VIEW view_buildable_derivations IS 'Shows all derivations ready to be claimed by workers, sorted by priority (newest commits first, smallest systems first within commits)';

-- Step 5: Create monitoring view for Grafana
CREATE OR REPLACE VIEW view_build_queue_status AS
WITH system_progress AS (
    SELECT
        d.id AS nixos_id,
        d.derivation_name AS system_name,
        c.commit_timestamp,
        c.git_commit_hash,
        COUNT(DISTINCT p.id) AS total_packages,
        COUNT(DISTINCT p.id) FILTER (WHERE p.status_id = 6) AS completed_packages,
        COUNT(DISTINCT p.id) FILTER (WHERE p.status_id = 8) AS building_packages,
        COUNT(DISTINCT br.id) AS active_workers,
        ARRAY_AGG(DISTINCT br.worker_id) FILTER (WHERE br.worker_id IS NOT NULL) AS worker_ids,
        MIN(br.reserved_at) AS earliest_reservation,
        MAX(br.heartbeat_at) AS latest_heartbeat
    FROM
        derivations d
        JOIN commits c ON c.id = d.commit_id
        LEFT JOIN derivation_dependencies dd ON dd.derivation_id = d.id
        LEFT JOIN derivations p ON p.id = dd.depends_on_id
            AND p.derivation_type = 'package'
        LEFT JOIN build_reservations br ON br.nixos_derivation_id = d.id
    WHERE
        d.derivation_type = 'nixos'
        AND d.status_id IN (5, 12) -- DryRunComplete, Scheduled
    GROUP BY
        d.id,
        d.derivation_name,
        c.commit_timestamp,
        c.git_commit_hash
)
SELECT
    nixos_id,
    system_name,
    commit_timestamp,
    git_commit_hash,
    total_packages,
    completed_packages,
    building_packages,
    (total_packages - completed_packages - building_packages) AS pending_packages,
    active_workers,
    worker_ids,
    earliest_reservation,
    latest_heartbeat,
    CASE WHEN total_packages = completed_packages THEN
        'ready_for_system_build'
    WHEN active_workers > 0 THEN
        'building'
    ELSE
        'pending'
    END AS status,
    CASE WHEN latest_heartbeat IS NOT NULL
        AND latest_heartbeat < NOW() - INTERVAL '5 minutes' THEN
        TRUE
    ELSE
        FALSE
    END AS has_stale_workers
FROM
    system_progress
ORDER BY
    commit_timestamp DESC,
    total_packages ASC;

COMMENT ON VIEW view_build_queue_status IS 'Monitoring view showing build progress for each NixOS system - for Grafana dashboards';


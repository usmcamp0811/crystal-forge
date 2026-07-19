DROP VIEW IF EXISTS view_buildable_derivations;

CREATE VIEW view_buildable_derivations AS
WITH system_progress AS (
    -- For each NixOS system, calculate total and completed packages
    SELECT
        r.nixos_id,
        r.nixos_commit_ts,
        COUNT(DISTINCT p.id) AS total_packages,
        COUNT(DISTINCT p.id) FILTER (WHERE p.status_id = 10) AS completed_packages, -- ← FIXED: 10 = BuildComplete
        COUNT(DISTINCT cpj.id) FILTER (WHERE cpj.status = 'completed') AS cached_packages,
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
            AND d.status_id IN (5, 7) -- ← FIXED: 5=DryRunComplete, 7=BuildPending
            AND d.derivation_path IS NOT NULL -- ← ADDED
) r
        LEFT JOIN derivation_dependencies dd ON dd.derivation_id = r.nixos_id
        LEFT JOIN derivations p ON p.id = dd.depends_on_id
            AND p.derivation_type = 'package'
        LEFT JOIN cache_push_jobs cpj ON cpj.derivation_id = p.id
            AND cpj.status = 'completed'
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
        sp.cached_packages,
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
        AND p.status_id IN (5, 7) -- ← FIXED: 5=DryRunComplete, 7=BuildPending
        AND p.derivation_path IS NOT NULL -- ← ADDED
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
        sp.cached_packages,
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
        AND d.status_id IN (5, 7) -- ← FIXED: 5=DryRunComplete, 7=BuildPending
        AND d.derivation_path IS NOT NULL -- ← ADDED
        AND d.attempt_count <= 5
        AND br.id IS NULL
        AND sp.total_packages = sp.completed_packages -- All deps built
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


-- Migration: Update view_system_deployment_status to use store_path
-- This view cannot use CREATE OR REPLACE due to column name changes
-- Must drop and recreate
DROP VIEW IF EXISTS view_system_deployment_status CASCADE;

CREATE VIEW view_system_deployment_status AS
WITH latest_system_states AS (
    SELECT DISTINCT ON (hostname)
        hostname,
        store_path,
        timestamp AS deployment_time
    FROM
        system_states
    ORDER BY
        hostname,
        timestamp DESC
),
system_current_derivations AS (
    SELECT
        lss.hostname,
        lss.store_path,
        lss.deployment_time,
        d.id AS derivation_id,
        d.commit_id AS current_commit_id,
        c.git_commit_hash AS current_commit_hash,
        c.commit_timestamp AS current_commit_timestamp,
        c.flake_id,
        f.name AS flake_name
    FROM
        latest_system_states lss
        LEFT JOIN derivations d ON lss.store_path = d.store_path
        LEFT JOIN commits c ON d.commit_id = c.id
        LEFT JOIN flakes f ON c.flake_id = f.id
),
latest_flake_commits AS (
    SELECT DISTINCT ON (s.hostname)
        s.hostname,
        c.id AS latest_commit_id,
        c.git_commit_hash AS latest_commit_hash,
        c.commit_timestamp AS latest_commit_timestamp,
        c.flake_id
    FROM
        systems s
        JOIN flakes f ON s.flake_id = f.id
        JOIN commits c ON f.id = c.flake_id
    ORDER BY
        s.hostname,
        c.commit_timestamp DESC
),
commit_counts AS (
    SELECT
        scd.hostname,
        COUNT(newer_commits.id) AS commits_behind
FROM
    system_current_derivations scd
    JOIN latest_flake_commits lfc ON scd.hostname = lfc.hostname
        LEFT JOIN commits newer_commits ON (newer_commits.flake_id = lfc.flake_id
                AND newer_commits.commit_timestamp > scd.current_commit_timestamp
                AND newer_commits.commit_timestamp <= lfc.latest_commit_timestamp)
    WHERE
        scd.current_commit_id IS NOT NULL
        AND lfc.latest_commit_id IS NOT NULL
    GROUP BY
        scd.hostname
)
SELECT
    COALESCE(s.hostname, scd.hostname, lfc.hostname) AS hostname,
    scd.store_path AS current_store_path,
    scd.deployment_time,
    scd.current_commit_hash,
    scd.current_commit_timestamp,
    lfc.latest_commit_hash,
    lfc.latest_commit_timestamp,
    COALESCE(cc.commits_behind, 0) AS commits_behind,
    scd.flake_name,
    CASE
    -- No deployment: system exists but no system_state
    WHEN scd.hostname IS NULL THEN
        'no_deployment'
        -- Unknown: has deployment but can't relate to any flake/commit
    WHEN scd.flake_id IS NULL THEN
        'unknown'
        -- Up to date: running the latest commit
    WHEN scd.current_commit_id = lfc.latest_commit_id THEN
        'up_to_date'
        -- Behind: running an older commit
    WHEN scd.current_commit_id != lfc.latest_commit_id
        AND scd.current_commit_timestamp < lfc.latest_commit_timestamp THEN
        'behind'
        -- Edge case: somehow running a newer commit than latest (shouldn't happen)
    WHEN scd.current_commit_timestamp > lfc.latest_commit_timestamp THEN
        'ahead'
    ELSE
        'unknown'
    END AS deployment_status,
    CASE WHEN scd.hostname IS NULL THEN
        'System registered but never deployed'
    WHEN scd.flake_id IS NULL THEN
        'Cannot determine flake relationship'
    WHEN scd.current_commit_id = lfc.latest_commit_id THEN
        'Running latest commit'
    WHEN scd.current_commit_id != lfc.latest_commit_id
        AND scd.current_commit_timestamp < lfc.latest_commit_timestamp THEN
        CONCAT('Behind by ', COALESCE(cc.commits_behind, 0), ' commit(s)')
    WHEN scd.current_commit_timestamp > lfc.latest_commit_timestamp THEN
        'Running newer commit than expected'
    ELSE
        'Deployment status unclear'
    END AS status_description
FROM
    systems s
    FULL OUTER JOIN system_current_derivations scd ON s.hostname = scd.hostname
    LEFT JOIN latest_flake_commits lfc ON COALESCE(s.hostname, scd.hostname) = lfc.hostname
    LEFT JOIN commit_counts cc ON COALESCE(s.hostname, scd.hostname) = cc.hostname
WHERE
    s.is_active = TRUE
    OR s.is_active IS NULL
ORDER BY
    CASE
    -- No deployment: system exists but no system_state
    WHEN scd.hostname IS NULL THEN
        1
        -- Unknown: has deployment but can't relate to any flake/commit
    WHEN scd.flake_id IS NULL THEN
        2
        -- Behind: running an older commit
    WHEN scd.current_commit_id != lfc.latest_commit_id
        AND scd.current_commit_timestamp < lfc.latest_commit_timestamp THEN
        3
        -- Up to date: running the latest commit
    WHEN scd.current_commit_id = lfc.latest_commit_id THEN
        4
        -- Edge case: somehow running a newer commit than latest
    WHEN scd.current_commit_timestamp > lfc.latest_commit_timestamp THEN
        5
    ELSE
        6
    END,
    commits_behind DESC NULLS LAST,
    hostname;


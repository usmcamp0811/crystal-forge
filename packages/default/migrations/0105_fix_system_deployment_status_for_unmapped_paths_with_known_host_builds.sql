-- Follow-up fix for deployment status classification.
--
-- If a host reports a current system path that cannot be mapped directly to a
-- derivation row, but the host does have known successful nixos builds, classify
-- as 'behind' (not 'unknown').
--
-- This keeps 'unknown' for hosts with no known successful build lineage.

CREATE OR REPLACE VIEW public.view_system_deployment_status AS
WITH latest_system_states AS (
    SELECT DISTINCT ON (hostname)
        hostname,
        store_path,
        timestamp AS deployment_time
    FROM system_states
    ORDER BY hostname, timestamp DESC
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
    FROM latest_system_states lss
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
    FROM systems s
    JOIN flakes f ON s.flake_id = f.id
    JOIN commits c ON f.id = c.flake_id
    ORDER BY s.hostname, c.commit_timestamp DESC
),
latest_expected_paths AS (
    SELECT DISTINCT ON (lfc.hostname)
        lfc.hostname,
        d.store_path AS expected_store_path
    FROM latest_flake_commits lfc
    JOIN derivations d
      ON d.commit_id = lfc.latest_commit_id
     AND d.derivation_name = lfc.hostname
     AND d.derivation_type = 'nixos'
     AND d.status_id = 10
     AND d.store_path IS NOT NULL
    ORDER BY lfc.hostname, d.completed_at DESC NULLS LAST, d.id DESC
),
known_host_paths AS (
    SELECT DISTINCT
        d.derivation_name AS hostname,
        d.store_path
    FROM derivations d
    WHERE d.derivation_type = 'nixos'
      AND d.status_id = 10
      AND d.store_path IS NOT NULL
),
known_host_builds AS (
    SELECT DISTINCT hostname FROM known_host_paths
),
commit_counts AS (
    SELECT
        scd.hostname,
        COUNT(newer_commits.id) AS commits_behind
    FROM system_current_derivations scd
    JOIN latest_flake_commits lfc ON scd.hostname = lfc.hostname
    LEFT JOIN commits newer_commits
      ON newer_commits.flake_id = lfc.flake_id
     AND newer_commits.commit_timestamp > scd.current_commit_timestamp
     AND newer_commits.commit_timestamp <= lfc.latest_commit_timestamp
    WHERE scd.current_commit_id IS NOT NULL
      AND lfc.latest_commit_id IS NOT NULL
    GROUP BY scd.hostname
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
        WHEN scd.hostname IS NULL THEN 'no_deployment'

        -- Primary commit-aware classification
        WHEN scd.flake_id IS NOT NULL AND scd.current_commit_id = lfc.latest_commit_id THEN 'up_to_date'
        WHEN scd.flake_id IS NOT NULL
             AND scd.current_commit_id != lfc.latest_commit_id
             AND scd.current_commit_timestamp < lfc.latest_commit_timestamp THEN 'behind'
        WHEN scd.flake_id IS NOT NULL
             AND scd.current_commit_timestamp > lfc.latest_commit_timestamp THEN 'ahead'

        -- Fallback expected latest path
        WHEN ep.expected_store_path IS NOT NULL AND scd.store_path = ep.expected_store_path THEN 'up_to_date'

        -- Fallback known lineage for host (includes unmapped current path)
        WHEN khp.store_path IS NOT NULL THEN 'behind'
        WHEN khb.hostname IS NOT NULL AND scd.store_path IS NOT NULL THEN 'behind'

        ELSE 'unknown'
    END AS deployment_status,
    CASE
        WHEN scd.hostname IS NULL THEN 'System registered but never deployed'

        WHEN scd.flake_id IS NOT NULL AND scd.current_commit_id = lfc.latest_commit_id THEN 'Running latest commit'
        WHEN scd.flake_id IS NOT NULL
             AND scd.current_commit_id != lfc.latest_commit_id
             AND scd.current_commit_timestamp < lfc.latest_commit_timestamp
            THEN CONCAT('Behind by ', COALESCE(cc.commits_behind, 0), ' commit(s)')
        WHEN scd.flake_id IS NOT NULL
             AND scd.current_commit_timestamp > lfc.latest_commit_timestamp
            THEN 'Running newer commit than expected'

        WHEN ep.expected_store_path IS NOT NULL AND scd.store_path = ep.expected_store_path
            THEN 'Running latest expected system build output'
        WHEN khp.store_path IS NOT NULL
            THEN 'Running a known older system build output'
        WHEN khb.hostname IS NOT NULL AND scd.store_path IS NOT NULL
            THEN 'Running an unmapped system output; host has newer known build outputs'

        ELSE 'Cannot determine flake relationship'
    END AS status_description
FROM systems s
FULL OUTER JOIN system_current_derivations scd
  ON s.hostname = scd.hostname
LEFT JOIN latest_flake_commits lfc
  ON COALESCE(s.hostname, scd.hostname) = lfc.hostname
LEFT JOIN latest_expected_paths ep
  ON COALESCE(s.hostname, scd.hostname) = ep.hostname
LEFT JOIN known_host_paths khp
  ON khp.hostname = COALESCE(s.hostname, scd.hostname)
 AND khp.store_path = scd.store_path
LEFT JOIN known_host_builds khb
  ON khb.hostname = COALESCE(s.hostname, scd.hostname)
LEFT JOIN commit_counts cc
  ON COALESCE(s.hostname, scd.hostname) = cc.hostname
WHERE s.is_active = TRUE OR s.is_active IS NULL
ORDER BY
    CASE
        WHEN scd.hostname IS NULL THEN 1
        WHEN ep.expected_store_path IS NOT NULL AND scd.store_path = ep.expected_store_path THEN 4
        WHEN khp.store_path IS NOT NULL THEN 3
        WHEN khb.hostname IS NOT NULL AND scd.store_path IS NOT NULL THEN 3
        WHEN scd.flake_id IS NOT NULL
             AND scd.current_commit_id != lfc.latest_commit_id
             AND scd.current_commit_timestamp < lfc.latest_commit_timestamp THEN 3
        WHEN scd.flake_id IS NOT NULL AND scd.current_commit_id = lfc.latest_commit_id THEN 4
        WHEN scd.flake_id IS NOT NULL AND scd.current_commit_timestamp > lfc.latest_commit_timestamp THEN 5
        ELSE 2
    END,
    commits_behind DESC NULLS LAST,
    hostname;

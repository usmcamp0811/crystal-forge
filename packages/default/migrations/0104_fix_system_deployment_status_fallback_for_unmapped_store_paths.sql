-- Fix deployment status classification when latest system_state store_path
-- cannot be mapped back to a derivation via direct store_path join.
--
-- Use COALESCE(d.store_path, d.expected_store_path) throughout so that:
-- 1. Pre-build: match against expected_store_path from eval
-- 2. Post-build: match against actual store_path from build
--
-- Fallback behavior:
-- - up_to_date: current store path equals latest expected/built nixos path for host
-- - behind: current store path matches a known built nixos path for host but is not expected
-- - unknown: current store path does not match any known built nixos path for host

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
    -- Match agent-reported store path against either the built path or the expected
    -- path so we can track alignment before the builder has completed.
    LEFT JOIN derivations d ON lss.store_path = COALESCE(d.store_path, d.expected_store_path)
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
    -- Get the expected (or actual) store path for the latest commit's derivation.
    -- Use COALESCE to prefer store_path (post-build) but fall back to expected_store_path (post-eval).
    SELECT DISTINCT ON (lfc.hostname)
        lfc.hostname,
        COALESCE(d.store_path, d.expected_store_path) AS expected_store_path
    FROM latest_flake_commits lfc
    JOIN derivations d
      ON d.commit_id = lfc.latest_commit_id
     AND d.derivation_name = lfc.hostname
     AND d.derivation_type = 'nixos'
     -- Require at least expected_store_path to be set (eval complete)
     AND COALESCE(d.store_path, d.expected_store_path) IS NOT NULL
    ORDER BY lfc.hostname, d.completed_at DESC NULLS LAST, d.id DESC
),
known_host_paths AS (
    -- All known store paths (expected or actual) for nixos derivations.
    SELECT DISTINCT
        d.derivation_name AS hostname,
        COALESCE(d.store_path, d.expected_store_path) AS store_path
    FROM derivations d
    WHERE d.derivation_type = 'nixos'
      AND COALESCE(d.store_path, d.expected_store_path) IS NOT NULL
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
        -- No deployment: system exists but no system_state
        WHEN scd.hostname IS NULL THEN 'no_deployment'

        -- Primary path: commit-aware comparison when current deployment maps cleanly
        WHEN scd.flake_id IS NOT NULL AND scd.current_commit_id = lfc.latest_commit_id THEN 'up_to_date'
        WHEN scd.flake_id IS NOT NULL
             AND scd.current_commit_id != lfc.latest_commit_id
             AND scd.current_commit_timestamp < lfc.latest_commit_timestamp THEN 'behind'
        WHEN scd.flake_id IS NOT NULL
             AND scd.current_commit_timestamp > lfc.latest_commit_timestamp THEN 'ahead'

        -- Fallback path: deployment does not map via direct store_path join
        WHEN ep.expected_store_path IS NOT NULL AND scd.store_path = ep.expected_store_path THEN 'up_to_date'
        WHEN khp.store_path IS NOT NULL THEN 'behind'

        -- Unknown: no derivation mapping and no known built path for this host
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
LEFT JOIN commit_counts cc
  ON COALESCE(s.hostname, scd.hostname) = cc.hostname
WHERE s.is_active = TRUE OR s.is_active IS NULL
ORDER BY
    CASE
        WHEN scd.hostname IS NULL THEN 1
        WHEN ep.expected_store_path IS NOT NULL AND scd.store_path = ep.expected_store_path THEN 4
        WHEN khp.store_path IS NOT NULL THEN 3
        WHEN scd.flake_id IS NOT NULL
             AND scd.current_commit_id != lfc.latest_commit_id
             AND scd.current_commit_timestamp < lfc.latest_commit_timestamp THEN 3
        WHEN scd.flake_id IS NOT NULL AND scd.current_commit_id = lfc.latest_commit_id THEN 4
        WHEN scd.flake_id IS NOT NULL AND scd.current_commit_timestamp > lfc.latest_commit_timestamp THEN 5
        ELSE 2
    END,
    commits_behind DESC NULLS LAST,
    hostname;

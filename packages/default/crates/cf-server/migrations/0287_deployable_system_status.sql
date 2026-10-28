-- Keep the existing eleven-column view contract: list and detail views depend
-- on this view and CREATE OR REPLACE must not change their column identities.
-- A target is deployable only when the registered flake/configuration has a
-- built NixOS path, both artifact policy flags, and a completed cache push.
-- Runtime deployment gates do not affect artifact deployability.
CREATE OR REPLACE VIEW public.view_system_deployment_status AS
WITH latest_system_states AS (
    SELECT DISTINCT ON (hostname)
        hostname, store_path, timestamp AS deployment_time
    FROM system_states
    ORDER BY hostname, timestamp DESC NULLS LAST, id DESC
), system_status AS (
    SELECT
        COALESCE(s.hostname, lss.hostname) AS hostname,
        lss.store_path AS current_store_path,
        lss.deployment_time,
        current_build.git_commit_hash AS current_commit_hash,
        current_build.commit_timestamp AS current_commit_timestamp,
        target.git_commit_hash AS latest_commit_hash,
        target.commit_timestamp AS latest_commit_timestamp,
        COALESCE(lag.commits_behind, 0::bigint) AS commits_behind,
        current_build.flake_name,
        CASE
            WHEN lss.hostname IS NULL THEN 'no_deployment'
            WHEN target.id IS NOT NULL AND lss.store_path = target.store_path
                THEN 'up_to_date'
            WHEN target.id IS NOT NULL
                 AND current_build.flake_id = s.flake_id
                 AND current_build.derivation_name = COALESCE(NULLIF(s.system_configuration_name, ''), s.hostname)
                 AND current_build.commit_timestamp > target.commit_timestamp
                THEN 'ahead'
            WHEN target.id IS NOT NULL
                 AND current_build.flake_id = s.flake_id
                 AND current_build.derivation_name = COALESCE(NULLIF(s.system_configuration_name, ''), s.hostname)
                THEN 'behind'
            ELSE 'unknown'
        END AS deployment_status
    FROM systems s
    FULL OUTER JOIN latest_system_states lss ON lss.hostname = s.hostname
    -- An unregistered state remains visible; inactive registered systems do not.
    LEFT JOIN LATERAL (
        SELECT d.id, d.store_path, c.id AS commit_id,
               c.git_commit_hash, c.commit_timestamp
        FROM commits c
        JOIN derivations d ON d.commit_id = c.id
        WHERE c.flake_id = s.flake_id
          AND c.source_archived = false
          AND d.derivation_type = 'nixos'
          AND d.derivation_name = COALESCE(NULLIF(s.system_configuration_name, ''), s.hostname)
          AND d.store_path IS NOT NULL AND BTRIM(d.store_path) <> ''
          AND d.cf_agent_enabled IS TRUE
          AND d.policy_requirements_met IS TRUE
          AND EXISTS (
              SELECT 1 FROM cache_push_jobs cpj
              WHERE cpj.derivation_id = d.id AND cpj.status = 'completed'
          )
        ORDER BY c.commit_timestamp DESC, d.completed_at DESC NULLS LAST, d.id DESC
        LIMIT 1
    ) target ON TRUE
    -- A store path can appear in multiple derivations. Pick a scoped mapping
    -- first, then use stable ordering so one state never multiplies view rows.
    LEFT JOIN LATERAL (
        SELECT d.id, d.derivation_name, c.flake_id, c.git_commit_hash,
               c.commit_timestamp, f.name AS flake_name
        FROM derivations d
        JOIN commits c ON c.id = d.commit_id
        JOIN flakes f ON f.id = c.flake_id
        WHERE lss.store_path = COALESCE(d.store_path, d.expected_store_path)
        ORDER BY (c.flake_id = s.flake_id AND
                  d.derivation_name = COALESCE(NULLIF(s.system_configuration_name, ''), s.hostname)) DESC NULLS LAST,
                 c.commit_timestamp DESC NULLS LAST, d.completed_at DESC NULLS LAST, d.id DESC
        LIMIT 1
    ) current_build ON TRUE
    LEFT JOIN LATERAL (
        -- Count only distinct commits with an eligible artifact for this host.
        -- Equal-timestamp artifact changes can be behind with a zero count.
        SELECT COUNT(DISTINCT c.id) AS commits_behind
        FROM commits c
        JOIN derivations d ON d.commit_id = c.id
        WHERE c.flake_id = s.flake_id
          AND c.source_archived = false
          AND c.commit_timestamp > current_build.commit_timestamp
          AND c.commit_timestamp <= target.commit_timestamp
          AND d.derivation_type = 'nixos'
          AND d.derivation_name = COALESCE(NULLIF(s.system_configuration_name, ''), s.hostname)
          AND d.store_path IS NOT NULL AND BTRIM(d.store_path) <> ''
          AND d.cf_agent_enabled IS TRUE
          AND d.policy_requirements_met IS TRUE
          AND EXISTS (
              SELECT 1 FROM cache_push_jobs cpj
              WHERE cpj.derivation_id = d.id AND cpj.status = 'completed'
          )
    ) lag ON TRUE
    WHERE s.is_active = TRUE OR s.id IS NULL
)
SELECT
    hostname,
    current_store_path,
    deployment_time,
    current_commit_hash,
    current_commit_timestamp,
    latest_commit_hash,
    latest_commit_timestamp,
    commits_behind,
    flake_name,
    deployment_status,
    CASE deployment_status
        WHEN 'no_deployment' THEN 'System registered but never deployed'
        WHEN 'up_to_date' THEN 'Running newest deployable system build'
        WHEN 'behind' THEN 'Running an older system build; a newer deployable build is available'
        WHEN 'ahead' THEN 'Running a newer commit than the newest deployable build'
        ELSE 'Cannot determine deployable flake relationship'
    END AS status_description
FROM system_status;

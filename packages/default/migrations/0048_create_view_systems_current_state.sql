CREATE OR REPLACE VIEW public.view_systems_current_state AS
WITH systems_latest_flake_commit AS (
    -- Get the latest commit for each system's flake
    SELECT DISTINCT ON (s.hostname)
        s.hostname,
        f.repo_url,
        c.git_commit_hash,
        c.commit_timestamp,
        c.id AS commit_id
    FROM
        public.systems s
        JOIN public.flakes f ON s.flake_id = f.id
        JOIN public.commits c ON f.id = c.flake_id
    ORDER BY
        s.hostname,
        c.commit_timestamp DESC
),
latest_system_state AS (
    -- Get the most recent system state for each hostname
    SELECT DISTINCT ON (system_states.hostname)
        system_states.hostname,
        system_states.derivation_path,
        system_states.primary_ip_address,
        system_states.uptime_secs,
        system_states.os,
        system_states.kernel,
        system_states.agent_version,
        system_states."timestamp"
    FROM
        public.system_states
    ORDER BY
        system_states.hostname,
        system_states."timestamp" DESC
),
latest_heartbeat AS (
    -- Get the most recent heartbeat for each system
    SELECT DISTINCT ON (ss.hostname)
        ss.hostname,
        ah."timestamp" AS last_heartbeat
    FROM
        public.system_states ss
        JOIN public.agent_heartbeats ah ON ah.system_state_id = ss.id
    ORDER BY
        ss.hostname,
        ah."timestamp" DESC
),
latest_evaluation AS (
    -- Get the evaluation status for the latest commit of each system
    SELECT
        d.derivation_name AS hostname,
        d.derivation_path,
        ds.name AS evaluation_status
    FROM
        public.derivations d
        JOIN public.derivation_statuses ds ON d.status_id = ds.id
        JOIN systems_latest_flake_commit slfc ON (d.commit_id = slfc.commit_id
                AND d.derivation_name = slfc.hostname)
    WHERE
        d.derivation_type = 'nixos'
        AND ds.name IN ('build-complete', 'complete'))
SELECT
    s.id AS system_id,
    s.hostname,
    slfc.repo_url,
    slfc.git_commit_hash AS deployed_commit_hash,
    slfc.commit_timestamp AS deployed_commit_timestamp,
    lss.derivation_path AS current_derivation_path,
    lss.primary_ip_address AS ip_address,
    ROUND((lss.uptime_secs::numeric / 86400::numeric), 1) AS uptime_days,
    lss.os,
    lss.kernel,
    lss.agent_version,
    lss."timestamp" AS last_deployed,
    le.derivation_path AS latest_commit_derivation_path,
    le.evaluation_status AS latest_commit_evaluation_status,
    CASE WHEN lss.derivation_path IS NOT NULL
        AND le.derivation_path IS NOT NULL
        AND lss.derivation_path = le.derivation_path THEN
        TRUE
    WHEN lss.derivation_path IS NOT NULL
        AND le.derivation_path IS NULL THEN
        FALSE -- Has deployment but no evaluation for latest commit = behind
    WHEN lss.derivation_path IS NOT NULL
        AND le.derivation_path IS NOT NULL
        AND lss.derivation_path != le.derivation_path THEN
        FALSE -- Has deployment and evaluation, but different = behind
    ELSE
        NULL::boolean -- No deployment or can't determine
    END AS is_running_latest_derivation,
    lhb.last_heartbeat,
    GREATEST (COALESCE(lss."timestamp", '1970-01-01 00:00:00'::timestamp with time zone), COALESCE(lhb.last_heartbeat, '1970-01-01 00:00:00'::timestamp with time zone)) AS last_seen
FROM
    public.systems s
    LEFT JOIN systems_latest_flake_commit slfc ON s.hostname = slfc.hostname
    LEFT JOIN latest_system_state lss ON s.hostname = lss.hostname
    LEFT JOIN latest_heartbeat lhb ON s.hostname = lhb.hostname
    LEFT JOIN latest_evaluation le ON s.hostname = le.hostname
ORDER BY
    s.hostname;


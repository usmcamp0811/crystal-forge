CREATE OR REPLACE VIEW view_systems_status_table AS
WITH latest_system_states AS (
    SELECT DISTINCT ON (hostname)
        id,
        hostname,
        derivation_path,
        change_reason,
        os,
        kernel,
        memory_gb,
        uptime_secs,
        cpu_brand,
        cpu_cores,
        primary_ip_address,
        nixos_version,
        agent_compatible,
        timestamp AS system_state_timestamp
    FROM
        system_states
    ORDER BY
        hostname,
        timestamp DESC
),
latest_heartbeats_per_hostname AS (
    SELECT DISTINCT ON (ss.hostname)
        ah.id AS heartbeat_id,
        ah.system_state_id AS heartbeat_system_state_id,
        ah.timestamp AS heartbeat_timestamp,
        ah.agent_version AS heartbeat_agent_version,
        ah.agent_build_hash AS heartbeat_agent_build_hash,
        ss.hostname
    FROM
        agent_heartbeats ah
        JOIN system_states ss ON ah.system_state_id = ss.id
    ORDER BY
        ss.hostname,
        ah.timestamp DESC
),
latest_flake_commits AS (
    SELECT DISTINCT ON (s.hostname)
        s.hostname,
        c.id AS latest_commit_id,
        c.git_commit_hash AS latest_commit_hash,
        c.commit_timestamp AS latest_commit_timestamp,
        d.derivation_path AS latest_derivation_path,
        d.id AS latest_derivation_id,
        ds.name AS latest_derivation_status,
        ds.is_success AS latest_derivation_success
    FROM
        systems s
        JOIN flakes f ON s.flake_id = f.id
        JOIN commits c ON f.id = c.flake_id
        LEFT JOIN derivations d ON (c.id = d.commit_id
                AND d.derivation_name = s.hostname
                AND d.derivation_type = 'nixos')
        LEFT JOIN derivation_statuses ds ON d.status_id = ds.id
    ORDER BY
        s.hostname,
        c.commit_timestamp DESC
),
system_registered_check AS (
    SELECT DISTINCT
        hostname
    FROM
        systems
    WHERE
        is_active = TRUE
)
SELECT
    COALESCE(lss.hostname, lfc.hostname, src.hostname) AS hostname,
    -- Connectivity Status
    CASE WHEN lss.hostname IS NULL THEN
        'never_seen'
    WHEN lhph.heartbeat_timestamp IS NULL
        AND lss.system_state_timestamp < NOW() - INTERVAL '30 minutes' THEN
        'offline'
    WHEN lhph.heartbeat_timestamp IS NULL
        AND lss.system_state_timestamp >= NOW() - INTERVAL '30 minutes' THEN
        'starting'
    WHEN lhph.heartbeat_timestamp < NOW() - INTERVAL '30 minutes'
        AND lss.system_state_timestamp < NOW() - INTERVAL '30 minutes' THEN
        'stale'
    WHEN lhph.heartbeat_timestamp < NOW() - INTERVAL '30 minutes'
        AND lss.system_state_timestamp >= NOW() - INTERVAL '30 minutes' THEN
        'starting'
    ELSE
        'online'
    END AS connectivity_status,
    -- Connectivity Status Text
    CASE WHEN lss.hostname IS NULL THEN
        'System registered but never seen'
    WHEN lhph.heartbeat_timestamp IS NULL
        AND lss.system_state_timestamp < NOW() - INTERVAL '30 minutes' THEN
        'No heartbeats'
    WHEN lhph.heartbeat_timestamp IS NULL
        AND lss.system_state_timestamp >= NOW() - INTERVAL '30 minutes' THEN
        'System starting up'
    WHEN lhph.heartbeat_timestamp < NOW() - INTERVAL '30 minutes'
        AND lss.system_state_timestamp < NOW() - INTERVAL '30 minutes' THEN
        'Heartbeat overdue'
    WHEN lhph.heartbeat_timestamp < NOW() - INTERVAL '30 minutes'
        AND lss.system_state_timestamp >= NOW() - INTERVAL '30 minutes' THEN
        'System restarted'
    ELSE
        'Active'
    END AS connectivity_status_text,
    -- Update Status (FIXED)
    CASE WHEN lss.hostname IS NULL THEN
        'never_seen'
    WHEN lfc.latest_derivation_success IS NOT TRUE THEN
        'evaluation_failed'
    WHEN lfc.latest_derivation_path IS NULL THEN
        'no_evaluation'
    WHEN lss.derivation_path IS NULL THEN
        'no_deployment'
    WHEN lss.derivation_path = lfc.latest_derivation_path THEN
        'up_to_date'
    WHEN lss.derivation_path != lfc.latest_derivation_path THEN
        'behind'
    ELSE
        'unknown'
    END AS update_status,
    -- Update Status Text (FIXED)
    CASE WHEN lss.hostname IS NULL THEN
        'System never deployed'
    WHEN lfc.latest_derivation_success IS NOT TRUE THEN
        'Latest commit evaluation failed (' || COALESCE(lfc.latest_derivation_status, 'unknown') || ')'
    WHEN lfc.latest_derivation_path IS NULL THEN
        'Latest commit not evaluated'
    WHEN lss.derivation_path IS NULL THEN
        'No current deployment'
    WHEN lss.derivation_path = lfc.latest_derivation_path THEN
        'Running latest version'
    WHEN lss.derivation_path != lfc.latest_derivation_path THEN
        'Behind latest commit'
    ELSE
        'Update status unknown'
    END AS update_status_text,
    -- Combined Status for quick overview (FIXED)
    CASE WHEN lss.hostname IS NULL THEN
        'never_seen'
    WHEN (lhph.heartbeat_timestamp IS NULL
        OR lhph.heartbeat_timestamp < NOW() - INTERVAL '30 minutes')
        AND lss.system_state_timestamp < NOW() - INTERVAL '30 minutes' THEN
        'offline'
    WHEN lfc.latest_derivation_success IS NOT TRUE THEN
        'evaluation_failed'
    WHEN lss.derivation_path IS NULL THEN
        'no_deployment'
    WHEN lss.derivation_path != lfc.latest_derivation_path THEN
        'behind'
    WHEN (lhph.heartbeat_timestamp IS NULL
        OR lhph.heartbeat_timestamp < NOW() - INTERVAL '30 minutes')
        AND lss.system_state_timestamp >= NOW() - INTERVAL '30 minutes' THEN
        'updating'
    ELSE
        'up_to_date'
    END AS overall_status,
    -- Rest of the view remains the same...
    GREATEST (COALESCE(lhph.heartbeat_timestamp, '1970-01-01'::timestamptz), COALESCE(lss.system_state_timestamp, '1970-01-01'::timestamptz))::text AS last_seen,
    COALESCE(lhph.heartbeat_agent_version, 'Unknown') AS agent_version,
    CASE WHEN lss.uptime_secs IS NOT NULL THEN
        EXTRACT(days FROM interval '1 second' * lss.uptime_secs)::text || 'd ' || EXTRACT(hours FROM interval '1 second' * lss.uptime_secs)::text || 'h'
    ELSE
        'Unknown'
    END AS uptime,
    COALESCE(lss.primary_ip_address, 'Unknown') AS ip_address,
    lss.os,
    lss.kernel,
    lss.nixos_version,
    lss.derivation_path AS current_derivation_path,
    lss.system_state_timestamp AS current_deployment_time,
    lfc.latest_commit_hash,
    lfc.latest_commit_timestamp,
    lfc.latest_derivation_path,
    lfc.latest_derivation_status,
    CASE WHEN lfc.latest_commit_timestamp IS NOT NULL
        AND lss.system_state_timestamp IS NOT NULL THEN
        ROUND(CAST(EXTRACT(EPOCH FROM (lfc.latest_commit_timestamp - lss.system_state_timestamp)) / 3600.0 AS numeric), 2)
    ELSE
        NULL
    END AS drift_hours
FROM
    system_registered_check src
    LEFT JOIN latest_system_states lss ON src.hostname = lss.hostname
    LEFT JOIN latest_heartbeats_per_hostname lhph ON src.hostname = lhph.hostname
    LEFT JOIN latest_flake_commits lfc ON src.hostname = lfc.hostname
ORDER BY
    CASE WHEN lss.hostname IS NULL THEN
        1
    WHEN (lhph.heartbeat_timestamp IS NULL
        OR lhph.heartbeat_timestamp < NOW() - INTERVAL '30 minutes')
        AND lss.system_state_timestamp < NOW() - INTERVAL '30 minutes' THEN
        2
    WHEN lfc.latest_derivation_success IS NOT TRUE THEN
        3
    WHEN lss.derivation_path IS NULL THEN
        4
    WHEN lss.derivation_path != lfc.latest_derivation_path THEN
        5
    WHEN (lhph.heartbeat_timestamp IS NULL
        OR lhph.heartbeat_timestamp < NOW() - INTERVAL '30 minutes')
        AND lss.system_state_timestamp >= NOW() - INTERVAL '30 minutes' THEN
        6
    ELSE
        7
    END,
    hostname;


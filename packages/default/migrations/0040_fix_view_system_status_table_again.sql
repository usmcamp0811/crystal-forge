DROP VIEW view_systems_status_table;

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
)
SELECT
    lss.hostname,
    CASE WHEN lhph.heartbeat_timestamp IS NULL
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
    END AS status,
    CASE WHEN lhph.heartbeat_timestamp IS NULL
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
    END AS status_text,
    GREATEST (COALESCE(lhph.heartbeat_timestamp, '1970-01-01'::timestamptz), lss.system_state_timestamp)::text AS last_seen,
    COALESCE(lhph.heartbeat_agent_version, 'Unknown') AS version,
    CASE WHEN lss.uptime_secs IS NOT NULL THEN
        EXTRACT(days FROM interval '1 second' * lss.uptime_secs)::text || 'd ' || EXTRACT(hours FROM interval '1 second' * lss.uptime_secs)::text || 'h'
    ELSE
        'Unknown'
    END AS uptime,
    COALESCE(lss.primary_ip_address, 'Unknown') AS ip_address
FROM
    latest_system_states lss
    LEFT JOIN latest_heartbeats_per_hostname lhph ON lss.hostname = lhph.hostname;


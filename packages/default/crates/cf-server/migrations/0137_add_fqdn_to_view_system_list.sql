-- Project fqdn column from systems into view_system_list so the
-- systems list table and side panel can display the persisted operator-managed
-- FQDN instead of falling back to the derived hostname.environment hostname.

CREATE OR REPLACE VIEW public.view_system_list AS
WITH latest_heartbeat AS (
    SELECT DISTINCT ON (s.id)
        s.id AS system_id,
        ah.timestamp AS heartbeat_timestamp
    FROM systems s
    LEFT JOIN system_states ss ON ss.hostname = s.hostname
    LEFT JOIN agent_heartbeats ah ON ah.system_state_id = ss.id
    ORDER BY s.id, ah.timestamp DESC NULLS LAST
),
latest_system_state AS (
    SELECT DISTINCT ON (s.id)
        s.id AS system_id,
        ss.primary_ip_address,
        ss.nixos_version,
        ss.timestamp AS state_timestamp
    FROM systems s
    LEFT JOIN system_states ss ON ss.hostname = s.hostname
    ORDER BY s.id, ss.timestamp DESC
),
deployment_info AS (
    SELECT
        hostname,
        deployment_status,
        status_description
    FROM view_system_deployment_status
),
cve_counts AS (
    SELECT
        v.hostname,
        COUNT(DISTINCT v.cve_id) FILTER (WHERE v.severity = 'CRITICAL')::integer AS critical_cve_count,
        COUNT(DISTINCT v.cve_id) FILTER (WHERE v.severity = 'HIGH')::integer    AS high_cve_count,
        COUNT(DISTINCT v.cve_id) FILTER (WHERE v.severity = 'MEDIUM')::integer  AS medium_cve_count,
        COUNT(DISTINCT v.cve_id) FILTER (WHERE v.severity = 'LOW')::integer     AS low_cve_count
    FROM view_system_vulnerabilities v
    JOIN systems s ON s.hostname = v.hostname
    WHERE s.is_active = TRUE
    GROUP BY v.hostname
)
SELECT
    s.id,
    s.hostname,
    e.name AS environment,
    lss.primary_ip_address,
    CASE
        WHEN lh.heartbeat_timestamp IS NULL THEN 'offline'
        WHEN lh.heartbeat_timestamp < NOW() - INTERVAL '4 hours' THEN 'offline'
        WHEN lh.heartbeat_timestamp < NOW() - INTERVAL '1 hour' THEN 'critical'
        WHEN lh.heartbeat_timestamp < NOW() - INTERVAL '15 minutes' THEN 'warning'
        ELSE 'healthy'
    END AS health_status,
    COALESCE(di.deployment_status, 'unknown') AS deployment_status,
    CASE
        WHEN s.flake_id IS NULL THEN 'unknown'
        WHEN lss.primary_ip_address IS NULL THEN 'ready_for_build'
        ELSE 'build_complete'
    END AS pipeline_stage,
    COALESCE(cc.critical_cve_count, 0)::integer AS critical_cve_count,
    COALESCE(cc.high_cve_count, 0)::integer     AS high_cve_count,
    COALESCE(cc.medium_cve_count, 0)::integer   AS medium_cve_count,
    COALESCE(cc.low_cve_count, 0)::integer      AS low_cve_count,
    lss.nixos_version,
    GREATEST(
        COALESCE(lh.heartbeat_timestamp, '1970-01-01'::timestamptz),
        COALESCE(lss.state_timestamp,    '1970-01-01'::timestamptz)
    ) AS last_seen,
    s.deployment_policy,
    s.fqdn
FROM systems s
LEFT JOIN environments e ON e.id = s.environment_id
LEFT JOIN latest_heartbeat lh ON lh.system_id = s.id
LEFT JOIN latest_system_state lss ON lss.system_id = s.id
LEFT JOIN deployment_info di ON di.hostname = s.hostname
LEFT JOIN cve_counts cc ON cc.hostname = s.hostname
WHERE s.is_active = TRUE;

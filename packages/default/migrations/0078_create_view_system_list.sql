-- Create system list view for API endpoints
-- This provides a lighter-weight view for list operations with key summary fields

CREATE OR REPLACE VIEW public.view_system_list AS
WITH latest_heartbeat AS (
    SELECT DISTINCT ON (s.id)
        s.id AS system_id,
        ah.timestamp AS heartbeat_timestamp
    FROM systems s
    LEFT JOIN system_states ss ON ss.hostname = s.hostname
    LEFT JOIN agent_heartbeats ah ON ah.system_state_id = ss.id
    ORDER BY s.id, ah.timestamp DESC
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
)
SELECT
    s.id,
    s.hostname,
    e.name AS environment,
    lss.primary_ip_address,
    -- Health status derived from heartbeat recency
    CASE
        WHEN lh.heartbeat_timestamp IS NULL THEN 'offline'
        WHEN lh.heartbeat_timestamp < NOW() - INTERVAL '4 hours' THEN 'offline'
        WHEN lh.heartbeat_timestamp < NOW() - INTERVAL '1 hour' THEN 'critical'
        WHEN lh.heartbeat_timestamp < NOW() - INTERVAL '15 minutes' THEN 'warning'
        ELSE 'healthy'
    END AS health_status,
    -- Deployment status
    COALESCE(di.deployment_status, 'unknown') AS deployment_status,
    -- Pipeline stage
    CASE
        WHEN s.flake_id IS NULL THEN 'unknown'
        WHEN lss.primary_ip_address IS NULL THEN 'ready_for_build'
        ELSE 'build_complete'
    END AS pipeline_stage,
    -- CVE counts (placeholder - could join with cve_scans)
    0::integer AS critical_cve_count,
    0::integer AS high_cve_count,
    0::integer AS medium_cve_count,
    0::integer AS low_cve_count,
    -- System info
    lss.nixos_version,
    GREATEST(
        COALESCE(lh.heartbeat_timestamp, '1970-01-01'::timestamptz),
        COALESCE(lss.state_timestamp, '1970-01-01'::timestamptz)
    ) AS last_seen,
    s.deployment_policy
FROM systems s
LEFT JOIN environments e ON e.id = s.environment_id
LEFT JOIN latest_heartbeat lh ON lh.system_id = s.id
LEFT JOIN latest_system_state lss ON lss.system_id = s.id
LEFT JOIN deployment_info di ON di.hostname = s.hostname
WHERE s.is_active = TRUE;

COMMENT ON VIEW public.view_system_list IS 
'Lightweight system list view for API list endpoints.
Provides summary information without heavy hardware details.';

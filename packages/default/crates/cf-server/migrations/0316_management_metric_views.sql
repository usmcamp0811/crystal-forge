-- Preserve view columns, types, and dependencies. Do not drop/cascade views.
-- Configuration evidence and guest observation absence are not host outages.
CREATE OR REPLACE VIEW public.view_system_list AS
WITH latest_heartbeat AS (
    SELECT s.id AS system_id, GREATEST(MAX(ah.timestamp), MAX(ss.timestamp)) AS heartbeat_timestamp
    FROM systems s
    LEFT JOIN system_states ss ON ss.hostname = s.hostname
    LEFT JOIN agent_heartbeats ah ON ah.system_state_id = ss.id
    GROUP BY s.id
), latest_system_state AS (
    SELECT DISTINCT ON (s.id) s.id AS system_id, ss.primary_ip_address,
        ss.nixos_version, ss.timestamp AS state_timestamp
    FROM systems s LEFT JOIN system_states ss ON ss.hostname = s.hostname
    ORDER BY s.id, ss.timestamp DESC
), cve_counts AS (
    SELECT s.hostname,
        COUNT(DISTINCT v.cve_id) FILTER (WHERE public.severity_from_cvss(c.cvss_v3_score) = 'CRITICAL')::integer AS critical_cve_count,
        COUNT(DISTINCT v.cve_id) FILTER (WHERE public.severity_from_cvss(c.cvss_v3_score) = 'HIGH')::integer AS high_cve_count,
        COUNT(DISTINCT v.cve_id) FILTER (WHERE public.severity_from_cvss(c.cvss_v3_score) = 'MEDIUM')::integer AS medium_cve_count,
        COUNT(DISTINCT v.cve_id) FILTER (WHERE public.severity_from_cvss(c.cvss_v3_score) = 'LOW')::integer AS low_cve_count
    FROM view_current_exact_cve_occurrences v JOIN systems s ON s.id = v.system_id
    JOIN cves c ON c.id = v.cve_id
    WHERE s.is_active AND s.management_mode = 'managed'
    GROUP BY s.hostname
)
SELECT s.id, s.hostname, e.name AS environment,
    CASE WHEN s.management_mode = 'managed' THEN lss.primary_ip_address END AS primary_ip_address,
    CASE
        WHEN s.management_mode <> 'managed' THEN 'unknown'
        WHEN lh.heartbeat_timestamp IS NULL THEN 'offline'
        WHEN lh.heartbeat_timestamp < NOW() - INTERVAL '4 hours' THEN 'offline'
        WHEN lh.heartbeat_timestamp < NOW() - INTERVAL '1 hour' THEN 'critical'
        WHEN lh.heartbeat_timestamp < NOW() - INTERVAL '15 minutes' THEN 'warning'
        ELSE 'healthy'
    END AS health_status,
    CASE WHEN s.management_mode = 'managed' THEN COALESCE(di.deployment_status, 'unknown')
        ELSE 'unknown' END AS deployment_status,
    CASE WHEN s.management_mode <> 'managed' OR s.flake_id IS NULL THEN 'unknown'
        WHEN lss.primary_ip_address IS NULL THEN 'ready_for_build'
        ELSE 'build_complete' END AS pipeline_stage,
    COALESCE(cc.critical_cve_count, 0)::integer AS critical_cve_count,
    COALESCE(cc.high_cve_count, 0)::integer AS high_cve_count,
    COALESCE(cc.medium_cve_count, 0)::integer AS medium_cve_count,
    COALESCE(cc.low_cve_count, 0)::integer AS low_cve_count,
    CASE WHEN s.management_mode = 'managed' THEN lss.nixos_version END AS nixos_version,
    CASE WHEN s.management_mode = 'managed' THEN GREATEST(
        COALESCE(lh.heartbeat_timestamp, '1970-01-01'::timestamptz),
        COALESCE(lss.state_timestamp, '1970-01-01'::timestamptz)) END AS last_seen,
    s.deployment_policy, s.fqdn, s.heartbeat_interval_secs, s.boot_id
FROM systems s
LEFT JOIN environments e ON e.id = s.environment_id
LEFT JOIN latest_heartbeat lh ON lh.system_id = s.id
LEFT JOIN latest_system_state lss ON lss.system_id = s.id
LEFT JOIN view_system_deployment_status di ON di.hostname = s.hostname
LEFT JOIN cve_counts cc ON cc.hostname = s.hostname
WHERE s.is_active;

COMMENT ON VIEW public.view_system_list IS
    'Active registrations. Only managed machines have runtime health/deployment/IP/version/last_seen. Nonmanaged health is unknown, never offline. CVE columns retain legacy integer compatibility; configuration evidence absence is represented by null counts in the configuration-assurance API.';

CREATE OR REPLACE VIEW public.view_environment_rollups AS
WITH managed AS (
    SELECT v.*, s.environment_id FROM view_system_list v JOIN systems s ON s.id=v.id
    WHERE s.management_mode='managed'
), env_cves AS (
    SELECT s.environment_id, COUNT(DISTINCT (s.id,v.cve_id))
        FILTER (WHERE public.severity_from_cvss(c.cvss_v3_score) IN ('CRITICAL','HIGH'))::bigint AS cve_count
    FROM systems s JOIN view_current_exact_cve_occurrences v ON v.system_id=s.id
    JOIN cves c ON c.id=v.cve_id
    WHERE s.is_active AND s.management_mode='managed' GROUP BY s.environment_id
), env_flakes AS (
    SELECT s.environment_id, array_agg(DISTINCT f.name) AS names
    FROM systems s JOIN flakes f ON f.id=s.flake_id WHERE s.is_active
    GROUP BY s.environment_id
)
SELECT e.id AS environment_id, COUNT(m.id)::bigint AS active_system_count,
    COUNT(m.id) FILTER (WHERE m.health_status='healthy')::bigint AS healthy_count,
    COUNT(m.id) FILTER (WHERE m.health_status='warning')::bigint AS warning_count,
    COUNT(m.id) FILTER (WHERE m.health_status='critical')::bigint AS critical_count,
    COUNT(m.id) FILTER (WHERE m.health_status='offline')::bigint AS offline_count,
    COALESCE(c.cve_count,0)::bigint AS cve_critical_high_count,
    COALESCE(f.names,ARRAY[]::text[]) AS flake_names
FROM environments e LEFT JOIN managed m ON m.environment_id=e.id
LEFT JOIN env_cves c ON c.environment_id=e.id
LEFT JOIN env_flakes f ON f.environment_id=e.id
GROUP BY e.id,c.cve_count,f.names;

COMMENT ON VIEW public.view_environment_rollups IS
    'Active managed-machine health partition and runtime CVEs. Flake names include active registrations in all modes. Environment API system_count separately counts every registration, including inactive resources.';

CREATE OR REPLACE VIEW public.view_fleet_health_status AS
SELECT CASE v.health_status WHEN 'healthy' THEN 'Healthy' WHEN 'warning' THEN 'Warning'
    WHEN 'critical' THEN 'Critical' ELSE 'Offline' END AS health_status, COUNT(*) AS count
FROM view_system_list v JOIN systems s ON s.id=v.id
WHERE s.management_mode='managed' GROUP BY v.health_status;

CREATE OR REPLACE VIEW public.view_deployment_status AS
SELECT COUNT(*) AS count, CASE v.deployment_status
    WHEN 'up_to_date' THEN 'Up to Date' WHEN 'ahead' THEN 'Up to Date'
    WHEN 'behind' THEN 'Behind' WHEN 'no_deployment' THEN 'No Deployment'
    ELSE 'Unknown' END AS status_display
FROM view_system_deployment_status v JOIN systems s ON s.hostname=v.hostname
WHERE s.is_active AND s.management_mode='managed'
GROUP BY CASE v.deployment_status
    WHEN 'up_to_date' THEN 'Up to Date' WHEN 'ahead' THEN 'Up to Date'
    WHEN 'behind' THEN 'Behind' WHEN 'no_deployment' THEN 'No Deployment'
    ELSE 'Unknown' END;

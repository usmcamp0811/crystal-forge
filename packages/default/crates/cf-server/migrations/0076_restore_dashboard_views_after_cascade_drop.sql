-- Restore dashboard-dependent views after CASCADE drops.

CREATE OR REPLACE VIEW public.view_systems_status_table AS
WITH latest_system_states AS (
    SELECT DISTINCT ON (ss.hostname)
        ss.hostname,
        ss.store_path,
        ss.primary_ip_address,
        ss.uptime_secs,
        ss.os,
        ss.kernel,
        ss.nixos_version,
        ss."timestamp" AS system_state_timestamp
    FROM public.system_states ss
    ORDER BY ss.hostname, ss."timestamp" DESC
),
latest_heartbeats_per_hostname AS (
    SELECT DISTINCT ON (ss.hostname)
        ss.hostname,
        ah."timestamp" AS heartbeat_timestamp,
        ah.agent_version AS heartbeat_agent_version
    FROM public.agent_heartbeats ah
    JOIN public.system_states ss ON ss.id = ah.system_state_id
    ORDER BY ss.hostname, ah."timestamp" DESC
),
deployments AS (
    SELECT
        d.hostname,
        d.current_store_path,
        d.deployment_status,
        d.status_description
    FROM public.view_system_deployment_status d
)
SELECT
    s.hostname,
    CASE
        WHEN lss.hostname IS NULL THEN 'never_seen'
        WHEN lhp.heartbeat_timestamp IS NULL
            AND lss.system_state_timestamp < NOW() - INTERVAL '30 minutes' THEN 'offline'
        WHEN lhp.heartbeat_timestamp IS NULL
            AND lss.system_state_timestamp >= NOW() - INTERVAL '30 minutes' THEN 'starting'
        WHEN lhp.heartbeat_timestamp < NOW() - INTERVAL '30 minutes'
            AND lss.system_state_timestamp < NOW() - INTERVAL '30 minutes' THEN 'stale'
        WHEN lhp.heartbeat_timestamp < NOW() - INTERVAL '30 minutes'
            AND lss.system_state_timestamp >= NOW() - INTERVAL '30 minutes' THEN 'starting'
        ELSE 'online'
    END AS connectivity_status,
    CASE
        WHEN lss.hostname IS NULL THEN 'System registered but never seen'
        WHEN lhp.heartbeat_timestamp IS NULL
            AND lss.system_state_timestamp < NOW() - INTERVAL '30 minutes' THEN 'No heartbeats'
        WHEN lhp.heartbeat_timestamp IS NULL
            AND lss.system_state_timestamp >= NOW() - INTERVAL '30 minutes' THEN 'System starting up'
        WHEN lhp.heartbeat_timestamp < NOW() - INTERVAL '30 minutes'
            AND lss.system_state_timestamp < NOW() - INTERVAL '30 minutes' THEN 'Heartbeat overdue'
        WHEN lhp.heartbeat_timestamp < NOW() - INTERVAL '30 minutes'
            AND lss.system_state_timestamp >= NOW() - INTERVAL '30 minutes' THEN 'System restarted'
        ELSE 'Active'
    END AS connectivity_status_text,
    CASE
        WHEN lss.hostname IS NULL THEN 'never_seen'
        WHEN dep.deployment_status = 'up_to_date' THEN 'up_to_date'
        WHEN dep.deployment_status = 'behind' THEN 'behind'
        WHEN dep.deployment_status = 'no_deployment' THEN 'no_deployment'
        WHEN dep.deployment_status = 'ahead' THEN 'up_to_date'
        WHEN dep.deployment_status = 'unknown' THEN 'unknown'
        ELSE 'unknown'
    END AS update_status,
    COALESCE(dep.status_description, 'Update status unknown') AS update_status_text,
    CASE
        WHEN lss.hostname IS NULL THEN 'never_seen'
        WHEN dep.deployment_status = 'behind' THEN 'behind'
        WHEN dep.deployment_status = 'no_deployment' THEN 'no_deployment'
        WHEN lhp.heartbeat_timestamp IS NULL
            OR lhp.heartbeat_timestamp < NOW() - INTERVAL '30 minutes' THEN 'offline'
        WHEN dep.deployment_status = 'up_to_date' THEN 'up_to_date'
        ELSE 'unknown'
    END AS overall_status,
    GREATEST(
        COALESCE(lhp.heartbeat_timestamp, '1970-01-01'::timestamptz),
        COALESCE(lss.system_state_timestamp, '1970-01-01'::timestamptz)
    )::text AS last_seen,
    COALESCE(lhp.heartbeat_agent_version, 'Unknown') AS agent_version,
    CASE
        WHEN lss.uptime_secs IS NOT NULL THEN
            EXTRACT(days FROM interval '1 second' * lss.uptime_secs)::text || 'd ' ||
            EXTRACT(hours FROM interval '1 second' * lss.uptime_secs)::text || 'h'
        ELSE 'Unknown'
    END AS uptime,
    COALESCE(lss.primary_ip_address, 'Unknown') AS ip_address,
    lss.os,
    lss.kernel,
    lss.nixos_version,
    lss.store_path AS current_derivation_path,
    lss.system_state_timestamp AS current_deployment_time,
    NULL::text AS latest_commit_hash,
    NULL::timestamptz AS latest_commit_timestamp,
    NULL::text AS latest_derivation_path,
    NULL::text AS latest_derivation_status,
    NULL::numeric AS drift_hours
FROM public.systems s
LEFT JOIN latest_system_states lss ON lss.hostname = s.hostname
LEFT JOIN latest_heartbeats_per_hostname lhp ON lhp.hostname = s.hostname
LEFT JOIN deployments dep ON dep.hostname = s.hostname
WHERE s.is_active = TRUE
ORDER BY s.hostname;

CREATE OR REPLACE VIEW public.view_fleet_health_status AS
SELECT
    health_status,
    COUNT(*) AS count
FROM (
    SELECT
        hostname,
        CASE
            WHEN last_seen::timestamptz > NOW() - INTERVAL '15 minutes' THEN 'Healthy'
            WHEN last_seen::timestamptz > NOW() - INTERVAL '1 hour' THEN 'Warning'
            WHEN last_seen::timestamptz > NOW() - INTERVAL '4 hours' THEN 'Critical'
            ELSE 'Offline'
        END AS health_status
    FROM public.view_systems_status_table
    WHERE last_seen IS NOT NULL AND last_seen != 'Unknown'
) health_data
GROUP BY health_status
ORDER BY CASE health_status
    WHEN 'Healthy' THEN 1
    WHEN 'Warning' THEN 2
    WHEN 'Critical' THEN 3
    WHEN 'Offline' THEN 4
    ELSE 5
END;

CREATE OR REPLACE VIEW public.view_deployment_status AS
SELECT
    COUNT(*) AS count,
    CASE update_status
        WHEN 'up_to_date' THEN 'Up to Date'
        WHEN 'behind' THEN 'Behind'
        WHEN 'evaluation_failed' THEN 'Evaluation Failed'
        WHEN 'no_evaluation' THEN 'No Evaluation'
        WHEN 'no_deployment' THEN 'No Deployment'
        WHEN 'never_seen' THEN 'Never Seen'
        WHEN 'unknown' THEN 'Unknown'
        ELSE 'Unknown'
    END AS status_display
FROM public.view_systems_status_table
GROUP BY update_status
ORDER BY CASE update_status
    WHEN 'up_to_date' THEN 1
    WHEN 'behind' THEN 2
    WHEN 'evaluation_failed' THEN 3
    WHEN 'no_evaluation' THEN 4
    WHEN 'no_deployment' THEN 5
    WHEN 'never_seen' THEN 6
    WHEN 'unknown' THEN 7
    ELSE 8
END;

CREATE OR REPLACE VIEW public.view_systems_cve_summary AS
WITH latest_scan_per_host AS (
    SELECT DISTINCT ON (d.derivation_name)
        d.derivation_name AS hostname,
        cs.completed_at,
        cs.scanner_name,
        cs.total_packages,
        cs.total_vulnerabilities,
        cs.critical_count,
        cs.high_count,
        cs.medium_count,
        cs.low_count
    FROM public.derivations d
    JOIN public.cve_scans cs ON cs.derivation_id = d.id
    WHERE d.derivation_type = 'nixos'
      AND cs.completed_at IS NOT NULL
    ORDER BY d.derivation_name, cs.completed_at DESC
)
SELECT
    s.hostname,
    dep.current_store_path AS current_derivation_path,
    dep.deployment_time AS last_deployed,
    st.last_seen::timestamptz AS last_seen,
    st.ip_address,
    ls.completed_at AS last_cve_scan,
    ls.scanner_name,
    COALESCE(ls.total_packages, 0) AS total_packages,
    COALESCE(ls.total_vulnerabilities, 0) AS total_cves,
    COALESCE(ls.critical_count, 0) AS critical_cves,
    COALESCE(ls.high_count, 0) AS high_cves,
    COALESCE(ls.medium_count, 0) AS medium_cves,
    COALESCE(ls.low_count, 0) AS low_cves,
    CASE
        WHEN COALESCE(ls.total_vulnerabilities, 0) = 0 THEN 'Clean'
        WHEN COALESCE(ls.critical_count, 0) > 0 THEN 'Critical'
        WHEN COALESCE(ls.high_count, 0) > 0 THEN 'High Risk'
        WHEN COALESCE(ls.medium_count, 0) > 0 THEN 'Medium Risk'
        WHEN COALESCE(ls.low_count, 0) > 0 THEN 'Low Risk'
        ELSE 'Unknown'
    END AS security_status,
    CASE
        WHEN ls.completed_at IS NOT NULL THEN EXTRACT(EPOCH FROM (NOW() - ls.completed_at)) / 86400
        ELSE NULL
    END AS days_since_scan
FROM public.systems s
LEFT JOIN public.view_systems_status_table st ON st.hostname = s.hostname
LEFT JOIN public.view_system_deployment_status dep ON dep.hostname = s.hostname
LEFT JOIN latest_scan_per_host ls ON ls.hostname = s.hostname
WHERE s.is_active = TRUE
ORDER BY s.hostname;

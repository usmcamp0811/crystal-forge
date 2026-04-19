-- Deduplicate CVE counts in view_system_list and view_system_detail.
--
-- Previously, the cve_counts CTE in both views used COUNT(*) against
-- view_system_vulnerabilities, which produces one row per (CVE, package,
-- derivation-path) combination. Because a single CVE can appear across
-- many package derivation paths on the same system (fanout), the same CVE
-- ID was counted multiple times, inflating critical/high/medium/low totals.
--
-- Fix: use COUNT(DISTINCT cve_id) FILTER (...) so each CVE ID is counted
-- at most once per system regardless of how many packages carry it.
--
-- This is a CREATE OR REPLACE VIEW so it is forward-safe and idempotent.
-- Migration 0110 is not modified.

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
        hostname,
        COUNT(DISTINCT cve_id) FILTER (WHERE severity = 'CRITICAL')::integer AS critical_cve_count,
        COUNT(DISTINCT cve_id) FILTER (WHERE severity = 'HIGH')::integer AS high_cve_count,
        COUNT(DISTINCT cve_id) FILTER (WHERE severity = 'MEDIUM')::integer AS medium_cve_count,
        COUNT(DISTINCT cve_id) FILTER (WHERE severity = 'LOW')::integer AS low_cve_count
    FROM view_system_vulnerabilities
    GROUP BY hostname
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
    COALESCE(cc.high_cve_count, 0)::integer AS high_cve_count,
    COALESCE(cc.medium_cve_count, 0)::integer AS medium_cve_count,
    COALESCE(cc.low_cve_count, 0)::integer AS low_cve_count,
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
LEFT JOIN cve_counts cc ON cc.hostname = s.hostname
WHERE s.is_active = TRUE;

CREATE OR REPLACE VIEW public.view_system_detail AS
WITH latest_system_state AS (
    SELECT DISTINCT ON (s.id)
        s.id AS system_id,
        ss.id AS system_state_id,
        ss.store_path,
        ss.os,
        ss.kernel,
        ss.nixos_version,
        ss.cpu_brand,
        ss.cpu_cores,
        ss.memory_gb,
        ss.uptime_secs,
        ss.board_serial,
        ss.chassis_serial,
        ss.bios_version,
        ss.cpu_microcode,
        ss.primary_ip_address,
        ss.primary_mac_address,
        ss.gateway_ip,
        ss.network_interfaces,
        ss.tpm_present,
        ss.secure_boot_enabled,
        ss.fips_mode,
        ss.selinux_status,
        ss.agent_version,
        ss.agent_build_hash,
        ss.timestamp AS last_seen
    FROM systems s
    LEFT JOIN system_states ss ON ss.hostname = s.hostname
    ORDER BY s.id, ss.timestamp DESC
),
latest_heartbeat AS (
    SELECT DISTINCT ON (s.id)
        s.id AS system_id,
        ah.timestamp AS heartbeat_timestamp,
        ah.agent_version AS heartbeat_agent_version
    FROM systems s
    LEFT JOIN system_states ss ON ss.hostname = s.hostname
    LEFT JOIN agent_heartbeats ah ON ah.system_state_id = ss.id
    ORDER BY s.id, ah.timestamp DESC NULLS LAST
),
hardware_change_detection AS (
    SELECT
        s.id AS system_id,
        EXISTS (
            SELECT 1
            FROM system_states ss_recent
            WHERE ss_recent.hostname = s.hostname
              AND ss_recent.timestamp >= NOW() - INTERVAL '24 hours'
              AND (
                  ss_recent.cpu_brand IS DISTINCT FROM lss.cpu_brand
                  OR ss_recent.cpu_cores IS DISTINCT FROM lss.cpu_cores
                  OR ss_recent.memory_gb IS DISTINCT FROM lss.memory_gb
                  OR ss_recent.board_serial IS DISTINCT FROM lss.board_serial
                  OR ss_recent.chassis_serial IS DISTINCT FROM lss.chassis_serial
                  OR ss_recent.bios_version IS DISTINCT FROM lss.bios_version
              )
        ) AS hardware_changed_24h,
        EXISTS (
            SELECT 1
            FROM (
                SELECT DISTINCT ON (ss_history.hostname)
                    ss_history.cpu_brand,
                    ss_history.cpu_cores,
                    ss_history.memory_gb,
                    ss_history.board_serial,
                    ss_history.chassis_serial,
                    ss_history.bios_version
                FROM system_states ss_history
                WHERE ss_history.hostname = s.hostname
                ORDER BY ss_history.hostname, ss_history.timestamp ASC
            ) first_state
            WHERE (
                first_state.cpu_brand IS DISTINCT FROM lss.cpu_brand
                OR first_state.cpu_cores IS DISTINCT FROM lss.cpu_cores
                OR first_state.memory_gb IS DISTINCT FROM lss.memory_gb
                OR first_state.board_serial IS DISTINCT FROM lss.board_serial
                OR first_state.chassis_serial IS DISTINCT FROM lss.chassis_serial
                OR first_state.bios_version IS DISTINCT FROM lss.bios_version
            )
        ) AS hardware_ever_changed
    FROM systems s
    LEFT JOIN latest_system_state lss ON lss.system_id = s.id
),
deployment_info AS (
    SELECT
        hostname,
        deployment_status,
        current_store_path,
        status_description
    FROM view_system_deployment_status
),
flake_info AS (
    SELECT
        s.id AS system_id,
        f.id AS flake_id,
        f.name AS flake_name,
        f.repo_url AS flake_repo_url,
        (
            SELECT c.git_commit_hash
            FROM commits c
            WHERE c.flake_id = f.id
            ORDER BY c.commit_timestamp DESC
            LIMIT 1
        ) AS latest_commit
    FROM systems s
    LEFT JOIN flakes f ON f.id = s.flake_id
),
latest_derivation AS (
    SELECT DISTINCT ON (d.derivation_name)
        d.derivation_name AS hostname,
        d.expected_store_path
    FROM derivations d
    WHERE d.derivation_type = 'nixos'
      AND d.expected_store_path IS NOT NULL
    ORDER BY d.derivation_name, d.completed_at DESC NULLS LAST, d.id DESC
),
cve_counts AS (
    SELECT
        hostname,
        COUNT(DISTINCT cve_id) FILTER (WHERE severity = 'CRITICAL')::integer AS critical_cve_count,
        COUNT(DISTINCT cve_id) FILTER (WHERE severity = 'HIGH')::integer AS high_cve_count,
        COUNT(DISTINCT cve_id) FILTER (WHERE severity = 'MEDIUM')::integer AS medium_cve_count,
        COUNT(DISTINCT cve_id) FILTER (WHERE severity = 'LOW')::integer AS low_cve_count
    FROM view_system_vulnerabilities
    GROUP BY hostname
)
SELECT
    s.id,
    s.hostname,
    e.name AS environment,
    s.is_active,
    s.deployment_policy,
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
        WHEN lss.store_path IS NULL THEN 'ready_for_build'
        ELSE 'build_complete'
    END AS pipeline_stage,
    lss.nixos_version,
    lss.kernel,
    lss.agent_version,
    lss.store_path AS current_store_path,
    lss.cpu_brand,
    lss.cpu_cores,
    lss.memory_gb,
    lss.uptime_secs,
    lss.board_serial,
    lss.bios_version,
    lss.primary_ip_address,
    lss.primary_mac_address,
    lss.gateway_ip,
    lss.tpm_present,
    lss.secure_boot_enabled,
    lss.fips_mode,
    lss.selinux_status,
    hcd.hardware_changed_24h,
    hcd.hardware_ever_changed,
    COALESCE(cc.critical_cve_count, 0)::integer AS critical_cve_count,
    COALESCE(cc.high_cve_count, 0)::integer AS high_cve_count,
    COALESCE(cc.medium_cve_count, 0)::integer AS medium_cve_count,
    COALESCE(cc.low_cve_count, 0)::integer AS low_cve_count,
    fi.flake_id,
    fi.flake_name,
    fi.flake_repo_url,
    fi.latest_commit AS flake_latest_commit,
    GREATEST(
        COALESCE(lh.heartbeat_timestamp, '1970-01-01'::timestamptz),
        COALESCE(lss.last_seen, '1970-01-01'::timestamptz)
    ) AS last_seen,
    s.created_at,
    s.updated_at,
    ld.expected_store_path
FROM systems s
LEFT JOIN environments e ON e.id = s.environment_id
LEFT JOIN latest_system_state lss ON lss.system_id = s.id
LEFT JOIN latest_heartbeat lh ON lh.system_id = s.id
LEFT JOIN hardware_change_detection hcd ON hcd.system_id = s.id
LEFT JOIN deployment_info di ON di.hostname = s.hostname
LEFT JOIN flake_info fi ON fi.system_id = s.id
LEFT JOIN latest_derivation ld ON ld.hostname = s.hostname
LEFT JOIN cve_counts cc ON cc.hostname = s.hostname;

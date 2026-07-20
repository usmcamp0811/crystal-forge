-- Fix last_heartbeat_at to include system_states timestamps, not just agent_heartbeats.
--
-- PROBLEM:
-- view_system_list and view_system_detail compute last_heartbeat_at from the
-- agent_heartbeats table only. However, heartbeats that trigger state changes
-- (e.g., reboots detected via boot_id) are written to system_states instead.
-- This causes systems to appear "offline" even though they're actively sending
-- heartbeats, because those heartbeats went to system_states, not agent_heartbeats.
--
-- FIX:
-- Update the latest_heartbeat CTE in both views to take the MAX of:
--   - agent_heartbeats.timestamp (lightweight heartbeats)
--   - system_states.timestamp (state-change heartbeats, including reboots)
--
-- This ensures last_heartbeat_at reflects the most recent contact from the agent
-- regardless of which table stored it.
--
-- NOTE: Must DROP views first because PostgreSQL cannot replace views when column
-- order changes. CASCADE will drop dependent views (e.g., view_environment_rollups).

-- ────────────────────────────────────────────────────────────────────────────
-- view_system_detail
-- ────────────────────────────────────────────────────────────────────────────

DROP VIEW IF EXISTS public.view_system_detail CASCADE;

CREATE VIEW public.view_system_detail AS
WITH latest_heartbeat AS (
    SELECT
        s.id AS system_id,
        -- Take the latest timestamp from either agent_heartbeats OR system_states.
        -- Both represent agent contact; the former is lightweight, the latter is
        -- state-change (including reboots).
        GREATEST(
            MAX(ah.timestamp),
            MAX(ss.timestamp)
        ) AS heartbeat_timestamp
    FROM systems s
    LEFT JOIN system_states ss ON ss.hostname = s.hostname
    LEFT JOIN agent_heartbeats ah ON ah.system_state_id = ss.id
    GROUP BY s.id
),
latest_system_state AS (
    SELECT DISTINCT ON (s.id)
        s.id AS system_id,
        ss.id AS state_id,
        ss.store_path,
        ss.generation,
        ss.generation_matches_current_store_path,
        ss.os,
        ss.kernel,
        ss.memory_gb,
        ss.uptime_secs,
        ss.cpu_brand,
        ss.cpu_cores,
        ss.board_serial,
        ss.product_uuid,
        ss.rootfs_uuid,
        ss.primary_mac_address,
        ss.primary_ip_address,
        ss.gateway_ip,
        ss.selinux_status,
        ss.tpm_present,
        ss.secure_boot_enabled,
        ss.fips_mode,
        ss.agent_version,
        ss.agent_build_hash,
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
    s.fqdn,
    e.name AS environment,
    f.name AS flake_name,
    s.system_configuration_name,
    lss.primary_ip_address,
    lss.store_path,
    lss.generation,
    lss.generation_matches_current_store_path,
    lss.os,
    lss.kernel,
    lss.memory_gb,
    lss.uptime_secs,
    lss.cpu_brand,
    lss.cpu_cores,
    lss.board_serial,
    lss.product_uuid,
    lss.rootfs_uuid,
    lss.primary_mac_address,
    lss.gateway_ip,
    lss.selinux_status,
    lss.tpm_present,
    lss.secure_boot_enabled,
    lss.fips_mode,
    lss.agent_version,
    lss.agent_build_hash,
    lss.nixos_version,
    -- Fixed fleet-health thresholds (restored by migration 0147).
    CASE
        WHEN lh.heartbeat_timestamp IS NULL THEN 'offline'
        WHEN lh.heartbeat_timestamp < NOW() - INTERVAL '4 hours'  THEN 'offline'
        WHEN lh.heartbeat_timestamp < NOW() - INTERVAL '1 hour'   THEN 'critical'
        WHEN lh.heartbeat_timestamp < NOW() - INTERVAL '15 minutes' THEN 'warning'
        ELSE 'healthy'
    END AS health_status,
    lh.heartbeat_timestamp AS last_heartbeat_at,
    di.deployment_status,
    di.status_description AS deployment_status_description,
    s.is_active,
    s.public_key,
    s.deployment_policy,
    s.desired_target,
    COALESCE(cc.critical_cve_count, 0) AS critical_cve_count,
    COALESCE(cc.high_cve_count, 0) AS high_cve_count,
    COALESCE(cc.medium_cve_count, 0) AS medium_cve_count,
    COALESCE(cc.low_cve_count, 0) AS low_cve_count,
    s.created_at,
    s.updated_at,
    COALESCE(s.heartbeat_interval_secs, 600) AS heartbeat_interval_secs,
    s.last_restart_type,
    s.last_restart_at
FROM systems s
LEFT JOIN environments e ON e.id = s.environment_id
LEFT JOIN flakes f ON f.id = s.flake_id
LEFT JOIN latest_heartbeat lh ON lh.system_id = s.id
LEFT JOIN latest_system_state lss ON lss.system_id = s.id
LEFT JOIN deployment_info di ON di.hostname = s.hostname
LEFT JOIN cve_counts cc ON cc.hostname = s.hostname;

COMMENT ON VIEW public.view_system_detail IS
    'Detailed per-system health and metadata for API and UI consumption. '
    'Fixed fleet-health thresholds (15min/1hr/4hr) restored by migration 0147. '
    'Heartbeat interval projection and restart classification added by migrations 0146, 0148, 0149. '
    'Fixed last_heartbeat_at to include system_states timestamps by migration 0151.';

-- ────────────────────────────────────────────────────────────────────────────
-- view_system_list
-- ────────────────────────────────────────────────────────────────────────────

DROP VIEW IF EXISTS public.view_system_list CASCADE;

CREATE VIEW public.view_system_list AS
WITH latest_heartbeat AS (
    SELECT
        s.id AS system_id,
        -- Take the latest timestamp from either agent_heartbeats OR system_states.
        GREATEST(
            MAX(ah.timestamp),
            MAX(ss.timestamp)
        ) AS heartbeat_timestamp
    FROM systems s
    LEFT JOIN system_states ss ON ss.hostname = s.hostname
    LEFT JOIN agent_heartbeats ah ON ah.system_state_id = ss.id
    GROUP BY s.id
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
    s.fqdn,
    e.name AS environment,
    lss.primary_ip_address,
    -- Fixed fleet-health thresholds (restored by migration 0147).
    CASE
        WHEN lh.heartbeat_timestamp IS NULL THEN 'offline'
        WHEN lh.heartbeat_timestamp < NOW() - INTERVAL '4 hours'  THEN 'offline'
        WHEN lh.heartbeat_timestamp < NOW() - INTERVAL '1 hour'   THEN 'critical'
        WHEN lh.heartbeat_timestamp < NOW() - INTERVAL '15 minutes' THEN 'warning'
        ELSE 'healthy'
    END AS health_status,
    lh.heartbeat_timestamp AS last_heartbeat_at,
    di.deployment_status,
    di.status_description AS deployment_status_description,
    s.is_active,
    s.public_key,
    s.deployment_policy,
    s.desired_target,
    lss.nixos_version,
    f.name AS flake_name,
    COALESCE(cc.critical_cve_count, 0) AS critical_cve_count,
    COALESCE(cc.high_cve_count, 0) AS high_cve_count,
    COALESCE(cc.medium_cve_count, 0) AS medium_cve_count,
    COALESCE(cc.low_cve_count, 0) AS low_cve_count,
    s.created_at,
    s.updated_at,
    COALESCE(s.heartbeat_interval_secs, 600) AS heartbeat_interval_secs
FROM systems s
LEFT JOIN environments e ON e.id = s.environment_id
LEFT JOIN flakes f ON f.id = s.flake_id
LEFT JOIN latest_heartbeat lh ON lh.system_id = s.id
LEFT JOIN latest_system_state lss ON lss.system_id = s.id
LEFT JOIN deployment_info di ON di.hostname = s.hostname
LEFT JOIN cve_counts cc ON cc.hostname = s.hostname;

COMMENT ON VIEW public.view_system_list IS
    'Per-system health summary for API list endpoints and UI cards. '
    'Fixed fleet-health thresholds (15min/1hr/4hr) restored by migration 0147. '
    'Heartbeat interval projection added by migration 0146. '
    'Fixed last_heartbeat_at to include system_states timestamps by migration 0151.';

-- ────────────────────────────────────────────────────────────────────────────
-- Recreate dependent views that were dropped by CASCADE
-- ────────────────────────────────────────────────────────────────────────────

-- view_environment_rollups depends on view_system_list, so recreate it.
-- Copy from migration 0140 (the latest version) but fix the latest_heartbeat
-- CTE to use GREATEST like the other views in this migration.
-- Use CREATE OR REPLACE in case CASCADE didn't fire due to partial failure.

DROP VIEW IF EXISTS public.view_environment_rollups;

CREATE VIEW public.view_environment_rollups AS
WITH latest_heartbeat AS (
    SELECT
        s.id AS system_id,
        -- Fixed: also include system_states timestamps (migration 0151/0150)
        GREATEST(
            MAX(ah.timestamp),
            MAX(ss.timestamp)
        ) AS heartbeat_timestamp
    FROM systems s
    LEFT JOIN system_states ss ON ss.hostname = s.hostname
    LEFT JOIN agent_heartbeats ah ON ah.system_state_id = ss.id
    GROUP BY s.id
),
system_health AS (
    SELECT
        s.id AS system_id,
        s.environment_id,
        s.flake_id,
        CASE
            WHEN lh.heartbeat_timestamp IS NULL THEN 'offline'
            WHEN lh.heartbeat_timestamp < NOW() - INTERVAL '4 hours' THEN 'offline'
            WHEN lh.heartbeat_timestamp < NOW() - INTERVAL '1 hour' THEN 'critical'
            WHEN lh.heartbeat_timestamp < NOW() - INTERVAL '15 minutes' THEN 'warning'
            ELSE 'healthy'
        END AS health_status
    FROM systems s
    LEFT JOIN latest_heartbeat lh ON lh.system_id = s.id
    WHERE s.is_active = TRUE
),
env_cve_counts AS (
    SELECT
        s.environment_id,
        COUNT(DISTINCT (s.id, v.cve_id))
            FILTER (WHERE v.severity IN ('CRITICAL', 'HIGH'))::bigint AS critical_high_cve_count
    FROM systems s
    JOIN view_system_vulnerabilities v ON v.hostname = s.hostname
    WHERE s.is_active = TRUE
    GROUP BY s.environment_id
),
env_flakes AS (
    SELECT
        sh.environment_id,
        COALESCE(
            array_agg(DISTINCT f.name) FILTER (WHERE f.name IS NOT NULL),
            ARRAY[]::text[]
        ) AS flake_names
    FROM system_health sh
    LEFT JOIN flakes f ON f.id = sh.flake_id
    GROUP BY sh.environment_id
)
SELECT
    e.id AS environment_id,
    COUNT(sh.system_id)::bigint AS active_system_count,
    COUNT(sh.system_id) FILTER (WHERE sh.health_status = 'healthy')::bigint  AS healthy_count,
    COUNT(sh.system_id) FILTER (WHERE sh.health_status = 'warning')::bigint  AS warning_count,
    COUNT(sh.system_id) FILTER (WHERE sh.health_status = 'critical')::bigint AS critical_count,
    COUNT(sh.system_id) FILTER (WHERE sh.health_status = 'offline')::bigint  AS offline_count,
    COALESCE(ecc.critical_high_cve_count, 0)::bigint AS cve_critical_high_count,
    COALESCE(ef.flake_names, ARRAY[]::text[]) AS flake_names
FROM environments e
LEFT JOIN system_health sh ON sh.environment_id = e.id
LEFT JOIN env_cve_counts ecc ON ecc.environment_id = e.id
LEFT JOIN env_flakes ef ON ef.environment_id = e.id
GROUP BY e.id, ecc.critical_high_cve_count, ef.flake_names;

COMMENT ON VIEW public.view_environment_rollups IS
    'Per-environment active-system health breakdown, critical+high CVE totals, and flake names. Derived from active systems; mirrors view_system_list health thresholds. Updated by TASK-358 migration 0140. Recreated by migration 0151 with fixed last_heartbeat_at.';

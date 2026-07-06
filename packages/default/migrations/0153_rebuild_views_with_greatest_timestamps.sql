-- Rebuild view_system_detail and view_system_list with all columns the Rust
-- code expects, using the GREATEST timestamp CTE for last_heartbeat_at.
--
-- PROBLEM:
-- Migrations 0151 and 0152 replaced both views with drastically simplified
-- versions that dropped many columns (pipeline_stage, last_seen, fqdn, boot_id,
-- hardware_change_detection, flake_info, etc.). The Rust SystemListRow and
-- SystemDetailRow structs expect these columns. The mismatch causes HTTP 500
-- errors at runtime when sqlx::FromRow cannot map the result set.
--
-- FIX:
-- Rebuild both views from the comprehensive 0149 (view_system_detail) and 0147
-- (view_system_list) definitions, but with the latest_heartbeat CTE fixed to
-- use GREATEST(MAX(ah.timestamp), MAX(ss.timestamp)) so that state-change
-- heartbeats written to system_states are not missed.
--
-- NOTE: Must DROP views first because PostgreSQL cannot replace views when
-- column order changes. CASCADE will drop dependent views.
--
-- NOTE: view_system_list explicitly does NOT include s.last_restart_type and
-- s.last_restart_at (by design from migration 0149's intent — the list view is
-- for cards/tables where restart classification is not displayed).

-- ────────────────────────────────────────────────────────────────────────────
-- view_system_detail
-- ────────────────────────────────────────────────────────────────────────────

DROP VIEW IF EXISTS public.view_system_detail CASCADE;

CREATE VIEW public.view_system_detail AS
WITH latest_system_state AS (
    SELECT DISTINCT ON (s.id)
        s.id AS system_id,
        ss.id AS system_state_id,
        ss.store_path,
        ss.generation,
        ss.generation_matches_current_store_path,
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
    ORDER BY s.id, ss.timestamp DESC NULLS LAST, ss.id DESC
),
latest_heartbeat AS (
    SELECT
        s.id AS system_id,
        GREATEST(
            MAX(ah.timestamp),
            MAX(ss.timestamp)
        ) AS heartbeat_timestamp,
        -- agent_version from the most recent agent_heartbeats row (best-effort)
        (SELECT ah2.agent_version
         FROM system_states ss2
         JOIN agent_heartbeats ah2 ON ah2.system_state_id = ss2.id
         WHERE ss2.hostname = s.hostname
         ORDER BY ah2.timestamp DESC
         LIMIT 1
        ) AS heartbeat_agent_version
    FROM systems s
    LEFT JOIN system_states ss ON ss.hostname = s.hostname
    LEFT JOIN agent_heartbeats ah ON ah.system_state_id = ss.id
    GROUP BY s.id
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
                  ss_recent.cpu_brand    IS DISTINCT FROM lss.cpu_brand
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
                first_state.cpu_brand    IS DISTINCT FROM lss.cpu_brand
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
    s.is_active,
    s.deployment_policy,
    -- Fixed fleet-health thresholds (restored from migration 0147).
    CASE
        WHEN lh.heartbeat_timestamp IS NULL THEN 'offline'
        WHEN lh.heartbeat_timestamp < NOW() - INTERVAL '4 hours'  THEN 'offline'
        WHEN lh.heartbeat_timestamp < NOW() - INTERVAL '1 hour'   THEN 'critical'
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
    COALESCE(cc.high_cve_count, 0)::integer     AS high_cve_count,
    COALESCE(cc.medium_cve_count, 0)::integer   AS medium_cve_count,
    COALESCE(cc.low_cve_count, 0)::integer      AS low_cve_count,
    fi.flake_id,
    fi.flake_name,
    fi.flake_repo_url,
    fi.latest_commit AS flake_latest_commit,
    GREATEST(
        COALESCE(lh.heartbeat_timestamp, '1970-01-01'::timestamptz),
        COALESCE(lss.last_seen,          '1970-01-01'::timestamptz)
    ) AS last_seen,
    s.created_at,
    s.updated_at,
    ld.expected_store_path,
    lss.generation,
    lss.generation_matches_current_store_path,
    s.reachability,
    s.fqdn,
    s.system_configuration_name,
    s.heartbeat_interval_secs,
    s.boot_id,
    -- Restart classification (added by migration 0148/0149).
    s.last_restart_type,
    s.last_restart_at
FROM systems s
LEFT JOIN environments e ON e.id = s.environment_id
LEFT JOIN latest_system_state lss ON lss.system_id = s.id
LEFT JOIN latest_heartbeat lh ON lh.system_id = s.id
LEFT JOIN hardware_change_detection hcd ON hcd.system_id = s.id
LEFT JOIN deployment_info di ON di.hostname = s.hostname
LEFT JOIN flake_info fi ON fi.system_id = s.id
LEFT JOIN latest_derivation ld ON ld.hostname = s.hostname
LEFT JOIN cve_counts cc ON cc.hostname = s.hostname;

COMMENT ON VIEW public.view_system_detail IS
    'Detailed per-system health and metadata for API and UI consumption. '
    'Fixed fleet-health thresholds (15min/1hr/4hr) restored by migration 0147. '
    'Heartbeat interval projection and restart classification added by migrations 0146, 0148, 0149. '
    'Rebuilt by migration 0153 with GREATEST timestamp CTE for last_heartbeat_at.';

-- ────────────────────────────────────────────────────────────────────────────
-- view_system_list
-- ────────────────────────────────────────────────────────────────────────────

DROP VIEW IF EXISTS public.view_system_list CASCADE;

CREATE VIEW public.view_system_list AS
WITH latest_heartbeat AS (
    SELECT
        s.id AS system_id,
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
    e.name AS environment,
    lss.primary_ip_address,
    -- Fixed fleet-health thresholds (restored from migration 0147).
    CASE
        WHEN lh.heartbeat_timestamp IS NULL THEN 'offline'
        WHEN lh.heartbeat_timestamp < NOW() - INTERVAL '4 hours'  THEN 'offline'
        WHEN lh.heartbeat_timestamp < NOW() - INTERVAL '1 hour'   THEN 'critical'
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
    s.fqdn,
    s.heartbeat_interval_secs,
    s.boot_id
FROM systems s
LEFT JOIN environments e ON e.id = s.environment_id
LEFT JOIN latest_heartbeat lh ON lh.system_id = s.id
LEFT JOIN latest_system_state lss ON lss.system_id = s.id
LEFT JOIN deployment_info di ON di.hostname = s.hostname
LEFT JOIN cve_counts cc ON cc.hostname = s.hostname
WHERE s.is_active = TRUE;

COMMENT ON VIEW public.view_system_list IS
    'Per-system health summary for API list endpoints and UI cards. '
    'Fixed fleet-health thresholds (15min/1hr/4hr) restored by migration 0147. '
    'Heartbeat interval projection and boot_id added by migration 0146. '
    'Rebuilt by migration 0153 with GREATEST timestamp CTE for last_heartbeat_at.';

-- ────────────────────────────────────────────────────────────────────────────
-- Recreate view_environment_rollups (was dropped by CASCADE from the DROP VIEW
-- ... CASCADE above)
-- ────────────────────────────────────────────────────────────────────────────

DROP VIEW IF EXISTS public.view_environment_rollups;

CREATE VIEW public.view_environment_rollups AS
WITH latest_heartbeat AS (
    SELECT
        s.id AS system_id,
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
    'Per-environment active-system health breakdown, critical+high CVE totals, and flake names. '
    'Rebuilt by migration 0153 with GREATEST timestamp CTE for last_heartbeat_at.';

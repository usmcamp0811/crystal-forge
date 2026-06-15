-- Per-environment rollups for the Environments surface (TASK-358).
--
-- Aggregates data that is cheaply derivable from the existing systems table so
-- the Environments list/cards/table can show a health breakdown, CVE totals,
-- and the set of flakes spanning each environment without N+1 queries.
--
-- Health-status thresholds intentionally mirror view_system_list so the
-- Environments surface stays consistent with the Systems surface.
--
-- Heavier per-environment attributes (cache assignment, gate policies,
-- compliance bundle, auto-sync, requires-approval, production flag, RBAC) are
-- NOT modeled here; they are tracked as follow-up tasks (TASK-359..TASK-362)
-- and rendered from clearly-commented UI placeholders until those land.

DROP VIEW IF EXISTS public.view_environment_rollups;

CREATE OR REPLACE VIEW public.view_environment_rollups AS
WITH latest_heartbeat AS (
    SELECT DISTINCT ON (s.id)
        s.id AS system_id,
        ah.timestamp AS heartbeat_timestamp
    FROM systems s
    LEFT JOIN system_states ss ON ss.hostname = s.hostname
    LEFT JOIN agent_heartbeats ah ON ah.system_state_id = ss.id
    ORDER BY s.id, ah.timestamp DESC NULLS LAST
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
    'Per-environment active-system health breakdown, critical+high CVE totals, and flake names. Derived from active systems; mirrors view_system_list health thresholds. Added by TASK-358.';

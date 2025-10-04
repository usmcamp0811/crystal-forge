CREATE OR REPLACE VIEW view_nixos_derivation_build_queue AS
WITH d_filtered AS (
    SELECT
        *
    FROM
        derivations
    WHERE
        status_id IN (5, 12)
),
roots AS (
    SELECT
        d.id AS nixos_id,
        c.commit_timestamp AS nixos_commit_ts
    FROM
        d_filtered d
        JOIN commits c ON c.id = d.commit_id
    WHERE
        d.derivation_type = 'nixos'
),
pkg_rows AS (
    SELECT
        p.*,
        r.nixos_id,
        r.nixos_commit_ts,
        0 AS group_order
    FROM
        roots r
        JOIN derivation_dependencies dd ON dd.derivation_id = r.nixos_id
        JOIN d_filtered p ON p.id = dd.depends_on_id
    WHERE
        p.derivation_type = 'package'
),
nixos_rows AS (
    SELECT
        n.*,
        r.nixos_id,
        r.nixos_commit_ts,
        1 AS group_order
    FROM
        roots r
        JOIN d_filtered n ON n.id = r.nixos_id
)
SELECT
    u.id,
    u.commit_id,
    u.derivation_type,
    u.derivation_name,
    u.derivation_path,
    u.scheduled_at,
    u.completed_at,
    u.attempt_count,
    u.started_at,
    u.evaluation_duration_ms,
    u.error_message,
    u.pname,
    u.version,
    u.status_id,
    u.derivation_target,
    u.build_elapsed_seconds,
    u.build_current_target,
    u.build_last_activity_seconds,
    u.build_last_heartbeat,
    u.cf_agent_enabled,
    u.store_path
FROM (
    SELECT
        *
    FROM
        pkg_rows
UNION ALL
SELECT
    *
FROM
    nixos_rows) AS u
ORDER BY
    u.nixos_commit_ts DESC,
    u.nixos_id,
    u.group_order, -- packages (0) first, then the nixos root (1)
    u.pname NULLS LAST,
    u.id;


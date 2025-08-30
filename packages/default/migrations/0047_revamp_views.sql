-- System heartbeat status view
-- Determines system health based on both heartbeats and system state changes
CREATE OR REPLACE VIEW view_system_heartbeat_status AS
WITH latest_system_states AS (
    SELECT DISTINCT ON (hostname)
        hostname,
        timestamp AS last_state_change,
        id AS system_state_id
    FROM
        system_states
    ORDER BY
        hostname,
        timestamp DESC
),
latest_heartbeats AS (
    SELECT DISTINCT ON (ss.hostname)
        ss.hostname,
        ah.timestamp AS last_heartbeat
    FROM
        system_states ss
        JOIN agent_heartbeats ah ON ah.system_state_id = ss.id
    ORDER BY
        ss.hostname,
        ah.timestamp DESC
),
heartbeat_analysis AS (
    SELECT
        COALESCE(lss.hostname, lhb.hostname) AS hostname,
        lss.last_state_change,
        lhb.last_heartbeat,
        GREATEST (COALESCE(lss.last_state_change, '1970-01-01'::timestamptz), COALESCE(lhb.last_heartbeat, '1970-01-01'::timestamptz)) AS most_recent_activity,
    CASE WHEN GREATEST (COALESCE(lss.last_state_change, '1970-01-01'::timestamptz), COALESCE(lhb.last_heartbeat, '1970-01-01'::timestamptz)) > NOW() - INTERVAL '15 minutes' THEN
        'Healthy'
    WHEN GREATEST (COALESCE(lss.last_state_change, '1970-01-01'::timestamptz), COALESCE(lhb.last_heartbeat, '1970-01-01'::timestamptz)) > NOW() - INTERVAL '1 hour' THEN
        'Warning'
    WHEN GREATEST (COALESCE(lss.last_state_change, '1970-01-01'::timestamptz), COALESCE(lhb.last_heartbeat, '1970-01-01'::timestamptz)) > NOW() - INTERVAL '4 hours' THEN
        'Critical'
    ELSE
        'Offline'
    END AS heartbeat_status,
    -- Calculate minutes since last activity for detailed info
    EXTRACT(EPOCH FROM (NOW() - GREATEST (COALESCE(lss.last_state_change, '1970-01-01'::timestamptz), COALESCE(lhb.last_heartbeat, '1970-01-01'::timestamptz)))) / 60 AS minutes_since_last_activity
FROM
    latest_system_states lss
    FULL OUTER JOIN latest_heartbeats lhb ON lss.hostname = lhb.hostname
)
SELECT
    hostname,
    heartbeat_status,
    most_recent_activity,
    last_heartbeat,
    last_state_change,
    ROUND(minutes_since_last_activity, 1) AS minutes_since_last_activity,
    CASE heartbeat_status
    WHEN 'Healthy' THEN
        'System is active and responding'
    WHEN 'Warning' THEN
        'System may be experiencing issues - no recent activity for 15–60 minutes'
    WHEN 'Critical' THEN
        'No activity for 1–4 hours'
    WHEN 'Offline' THEN
        'No activity for >4 hours'
    END AS status_description
FROM
    heartbeat_analysis
ORDER BY
    CASE heartbeat_status
    WHEN 'Offline' THEN
        1
    WHEN 'Critical' THEN
        2
    WHEN 'Warning' THEN
        3
    WHEN 'Healthy' THEN
        4
    END,
    minutes_since_last_activity DESC;

-- System deployment status view
-- Shows where each system stands relative to the latest commit in their flake
CREATE OR REPLACE VIEW view_system_deployment_status AS
WITH latest_system_states AS (
    SELECT DISTINCT ON (hostname)
        hostname,
        derivation_path,
        timestamp AS deployment_time
    FROM
        system_states
    ORDER BY
        hostname,
        timestamp DESC
),
system_current_derivations AS (
    SELECT
        lss.hostname,
        lss.derivation_path,
        lss.deployment_time,
        d.id AS derivation_id,
        d.commit_id AS current_commit_id,
        c.git_commit_hash AS current_commit_hash,
        c.commit_timestamp AS current_commit_timestamp,
        c.flake_id,
        f.name AS flake_name
    FROM
        latest_system_states lss
        LEFT JOIN derivations d ON lss.derivation_path = d.derivation_path
        LEFT JOIN commits c ON d.commit_id = c.id
        LEFT JOIN flakes f ON c.flake_id = f.id
),
latest_flake_commits AS (
    SELECT DISTINCT ON (s.hostname)
        s.hostname,
        c.id AS latest_commit_id,
        c.git_commit_hash AS latest_commit_hash,
        c.commit_timestamp AS latest_commit_timestamp,
        c.flake_id
    FROM
        systems s
        JOIN flakes f ON s.flake_id = f.id
        JOIN commits c ON f.id = c.flake_id
    ORDER BY
        s.hostname,
        c.commit_timestamp DESC
),
commit_counts AS (
    SELECT
        scd.hostname,
        COUNT(newer_commits.id) AS commits_behind
FROM
    system_current_derivations scd
    JOIN latest_flake_commits lfc ON scd.hostname = lfc.hostname
        LEFT JOIN commits newer_commits ON (newer_commits.flake_id = lfc.flake_id
                AND newer_commits.commit_timestamp > scd.current_commit_timestamp
                AND newer_commits.commit_timestamp <= lfc.latest_commit_timestamp)
    WHERE
        scd.current_commit_id IS NOT NULL
        AND lfc.latest_commit_id IS NOT NULL
    GROUP BY
        scd.hostname
)
SELECT
    COALESCE(s.hostname, scd.hostname, lfc.hostname) AS hostname,
    scd.derivation_path AS current_derivation_path,
    scd.deployment_time,
    scd.current_commit_hash,
    scd.current_commit_timestamp,
    lfc.latest_commit_hash,
    lfc.latest_commit_timestamp,
    COALESCE(cc.commits_behind, 0) AS commits_behind,
    scd.flake_name,
    CASE
    -- No deployment: system exists but no system_state
    WHEN scd.hostname IS NULL THEN
        'no_deployment'
        -- Unknown: has deployment but can't relate to any flake/commit
    WHEN scd.flake_id IS NULL THEN
        'unknown'
        -- Up to date: running the latest commit
    WHEN scd.current_commit_id = lfc.latest_commit_id THEN
        'up_to_date'
        -- Behind: running an older commit
    WHEN scd.current_commit_id != lfc.latest_commit_id
        AND scd.current_commit_timestamp < lfc.latest_commit_timestamp THEN
        'behind'
        -- Edge case: somehow running a newer commit than latest (shouldn't happen)
    WHEN scd.current_commit_timestamp > lfc.latest_commit_timestamp THEN
        'ahead'
    ELSE
        'unknown'
    END AS deployment_status,
    CASE WHEN scd.hostname IS NULL THEN
        'System registered but never deployed'
    WHEN scd.flake_id IS NULL THEN
        'Cannot determine flake relationship'
    WHEN scd.current_commit_id = lfc.latest_commit_id THEN
        'Running latest commit'
    WHEN scd.current_commit_id != lfc.latest_commit_id
        AND scd.current_commit_timestamp < lfc.latest_commit_timestamp THEN
        CONCAT('Behind by ', COALESCE(cc.commits_behind, 0), ' commit(s)')
    WHEN scd.current_commit_timestamp > lfc.latest_commit_timestamp THEN
        'Running newer commit than expected'
    ELSE
        'Deployment status unclear'
    END AS status_description
FROM
    systems s
    FULL OUTER JOIN system_current_derivations scd ON s.hostname = scd.hostname
    LEFT JOIN latest_flake_commits lfc ON COALESCE(s.hostname, scd.hostname) = lfc.hostname
    LEFT JOIN commit_counts cc ON COALESCE(s.hostname, scd.hostname) = cc.hostname
WHERE
    s.is_active = TRUE
    OR s.is_active IS NULL
ORDER BY
    CASE
    -- No deployment: system exists but no system_state
    WHEN scd.hostname IS NULL THEN
        1
        -- Unknown: has deployment but can't relate to any flake/commit
    WHEN scd.flake_id IS NULL THEN
        2
        -- Behind: running an older commit
    WHEN scd.current_commit_id != lfc.latest_commit_id
        AND scd.current_commit_timestamp < lfc.latest_commit_timestamp THEN
        3
        -- Up to date: running the latest commit
    WHEN scd.current_commit_id = lfc.latest_commit_id THEN
        4
        -- Edge case: somehow running a newer commit than latest
    WHEN scd.current_commit_timestamp > lfc.latest_commit_timestamp THEN
        5
    ELSE
        6
    END,
    commits_behind DESC NULLS LAST,
    hostname;

-- Commit build status view
-- Shows all commits with their derivation build status for filtering and analysis
CREATE OR REPLACE VIEW view_commit_build_status AS
WITH commit_derivation_summary AS (
    SELECT
        c.id AS commit_id,
        c.flake_id,
        c.git_commit_hash,
        c.commit_timestamp,
        c.attempt_count AS commit_attempt_count,
        f.name AS flake_name,
        f.repo_url,
        d.id AS derivation_id,
        d.derivation_type,
        d.derivation_name,
        d.derivation_path,
        d.scheduled_at AS derivation_scheduled_at,
        d.started_at AS derivation_started_at,
        d.completed_at AS derivation_completed_at,
        d.attempt_count AS derivation_attempt_count,
        d.evaluation_duration_ms,
        d.error_message,
        d.pname,
        d.version,
        ds.name AS derivation_status,
        ds.description AS derivation_status_description,
        ds.is_terminal,
        ds.is_success,
        ds.display_order
    FROM
        commits c
        JOIN flakes f ON c.flake_id = f.id
        LEFT JOIN derivations d ON c.id = d.commit_id
        LEFT JOIN derivation_statuses ds ON d.status_id = ds.id
),
derivation_counts AS (
    SELECT
        commit_id,
        COUNT(
            CASE WHEN derivation_id IS NOT NULL THEN
                1
            END) AS total_derivations,
    COUNT(
        CASE WHEN is_success = TRUE THEN
            1
        END) AS successful_derivations,
    COUNT(
        CASE WHEN is_success = FALSE
            AND is_terminal = TRUE THEN
            1
        END) AS failed_derivations,
    COUNT(
        CASE WHEN is_terminal = FALSE THEN
            1
        END) AS in_progress_derivations,
    COUNT(
        CASE WHEN derivation_type = 'nixos' THEN
            1
        END) AS nixos_derivations,
    COUNT(
        CASE WHEN derivation_type = 'package' THEN
            1
        END) AS package_derivations,
    AVG(
        CASE WHEN evaluation_duration_ms IS NOT NULL THEN
            evaluation_duration_ms
        END) AS avg_evaluation_duration_ms,
    MAX(derivation_attempt_count) AS max_derivation_attempts,
    STRING_AGG(DISTINCT derivation_status, ', ' ORDER BY derivation_status) AS all_statuses
FROM
    commit_derivation_summary
GROUP BY
    commit_id
)
SELECT
    cds.commit_id,
    cds.flake_name,
    cds.repo_url,
    cds.git_commit_hash,
    LEFT (cds.git_commit_hash,
        8) AS short_hash,
    cds.commit_timestamp,
    cds.commit_attempt_count,
    -- Individual derivation fields (when querying specific derivations)
    cds.derivation_id,
    cds.derivation_type,
    cds.derivation_name,
    cds.derivation_path,
    cds.derivation_scheduled_at,
    cds.derivation_started_at,
    cds.derivation_completed_at,
    cds.derivation_attempt_count,
    cds.evaluation_duration_ms,
    cds.error_message,
    cds.pname,
    cds.version,
    cds.derivation_status,
    cds.derivation_status_description,
    cds.is_terminal,
    cds.is_success,
    -- Commit-level aggregations
    COALESCE(dc.total_derivations, 0) AS total_derivations,
    COALESCE(dc.successful_derivations, 0) AS successful_derivations,
    COALESCE(dc.failed_derivations, 0) AS failed_derivations,
    COALESCE(dc.in_progress_derivations, 0) AS in_progress_derivations,
    COALESCE(dc.nixos_derivations, 0) AS nixos_derivations,
    COALESCE(dc.package_derivations, 0) AS package_derivations,
    ROUND(COALESCE(dc.avg_evaluation_duration_ms, 0) / 1000.0, 2) AS avg_evaluation_duration_seconds,
    COALESCE(dc.max_derivation_attempts, 0) AS max_derivation_attempts,
    dc.all_statuses,
    -- Commit-level status assessment
    CASE WHEN dc.total_derivations = 0 THEN
        'no_builds'
    WHEN dc.in_progress_derivations > 0 THEN
        'building'
    WHEN dc.failed_derivations > 0
        AND dc.successful_derivations = 0 THEN
        'failed'
    WHEN dc.failed_derivations > 0
        AND dc.successful_derivations > 0 THEN
        'partial'
    WHEN dc.successful_derivations = dc.total_derivations THEN
        'complete'
    ELSE
        'unknown'
    END AS commit_build_status,
    CASE WHEN dc.total_derivations = 0 THEN
        'No derivations scheduled for this commit'
    WHEN dc.in_progress_derivations > 0 THEN
        CONCAT(dc.in_progress_derivations, ' of ', dc.total_derivations, ' derivations still building')
    WHEN dc.failed_derivations > 0
        AND dc.successful_derivations = 0 THEN
        CONCAT('All ', dc.total_derivations, ' derivations failed')
    WHEN dc.failed_derivations > 0
        AND dc.successful_derivations > 0 THEN
        CONCAT(dc.successful_derivations, ' successful, ', dc.failed_derivations, ' failed of ', dc.total_derivations, ' total')
    WHEN dc.successful_derivations = dc.total_derivations THEN
        CONCAT('All ', dc.total_derivations, ' derivations completed successfully')
    ELSE
        'Build status unclear'
    END AS commit_build_description
FROM
    commit_derivation_summary cds
    LEFT JOIN derivation_counts dc ON cds.commit_id = dc.commit_id
ORDER BY
    cds.commit_timestamp DESC,
    cds.flake_name,
    cds.derivation_type,
    cds.derivation_name;

CREATE OR REPLACE VIEW public.view_commit_deployment_timeline AS
WITH commit_evaluations AS (
    -- Get evaluation status for each commit
    SELECT
        c.flake_id,
        c.id AS commit_id,
        c.git_commit_hash,
        c.commit_timestamp,
        COUNT(DISTINCT d.id) AS total_evaluations,
        COUNT(DISTINCT d.id) FILTER (WHERE ds.is_success = TRUE) AS successful_evaluations,
        STRING_AGG(DISTINCT ds.name, ', ') AS evaluation_statuses,
        STRING_AGG(DISTINCT d.derivation_name, ', ') AS evaluated_targets
    FROM
        commits c
        LEFT JOIN derivations d ON c.id = d.commit_id
            AND d.derivation_type = 'nixos'
        LEFT JOIN derivation_statuses ds ON d.status_id = ds.id
    WHERE
        c.commit_timestamp >= NOW() - INTERVAL '30 days'
    GROUP BY
        c.flake_id,
        c.id,
        c.git_commit_hash,
        c.commit_timestamp
),
actual_system_deployments AS (
    -- Find what systems are actually running based on system_states
    SELECT DISTINCT ON (ss.hostname)
        ss.hostname,
        ss.derivation_path,
        ss.timestamp AS deployment_time,
        d.commit_id,
        c.git_commit_hash,
        c.commit_timestamp
    FROM
        system_states ss
        LEFT JOIN derivations d ON ss.derivation_path = d.derivation_path
            AND d.derivation_type = 'nixos'
        LEFT JOIN commits c ON d.commit_id = c.id
    WHERE
        ss.timestamp >= NOW() - INTERVAL '30 days'
    ORDER BY
        ss.hostname,
        ss.timestamp DESC
),
latest_successful_by_system AS (
    -- For each system, find the most recent successful derivation
    SELECT DISTINCT ON (d.derivation_name)
        d.derivation_name,
        d.commit_id,
        c.git_commit_hash,
        c.commit_timestamp AS derivation_commit_time
    FROM
        derivations d
        JOIN derivation_statuses ds ON d.status_id = ds.id
        JOIN commits c ON d.commit_id = c.id
    WHERE
        d.derivation_type = 'nixos'
        AND ds.is_success = TRUE
        AND c.commit_timestamp >= NOW() - INTERVAL '30 days'
    ORDER BY
        d.derivation_name,
        c.commit_timestamp DESC
),
system_first_seen_with_commit AS (
    -- Find when we first saw each system after each commit was made
    -- This approximates "deployment time"
    SELECT
        lsbs.commit_id,
        lsbs.derivation_name,
        MIN(ss.timestamp) FILTER (WHERE ss.timestamp > lsbs.derivation_commit_time) AS first_seen_after_commit
    FROM
        latest_successful_by_system lsbs
        JOIN system_states ss ON ss.hostname = lsbs.derivation_name
    GROUP BY
        lsbs.commit_id,
        lsbs.derivation_name
),
current_deployments AS (
    -- Get current deployment status
    SELECT DISTINCT ON (hostname)
        hostname,
        timestamp AS current_timestamp
    FROM
        system_states
    ORDER BY
        hostname,
        timestamp DESC
),
commit_deployment_stats AS (
    -- Aggregate deployment info per commit
    SELECT
        lsbs.commit_id,
        COUNT(DISTINCT lsbs.derivation_name) AS systems_with_successful_evaluation,
        COUNT(DISTINCT sfswc.derivation_name) AS systems_seen_after_commit,
        MIN(sfswc.first_seen_after_commit) AS first_system_seen,
        MAX(sfswc.first_seen_after_commit) AS last_system_seen,
        STRING_AGG(DISTINCT sfswc.derivation_name, ', ') AS systems_seen_list,
        -- Current systems running this commit (based on latest successful derivation)
        COUNT(DISTINCT CASE WHEN cd.hostname IS NOT NULL THEN
                lsbs.derivation_name
            END) AS currently_active_systems,
        STRING_AGG(DISTINCT CASE WHEN cd.hostname IS NOT NULL THEN
                lsbs.derivation_name
            END, ', ') AS currently_active_systems_list
    FROM
        latest_successful_by_system lsbs
        LEFT JOIN system_first_seen_with_commit sfswc ON lsbs.commit_id = sfswc.commit_id
            AND lsbs.derivation_name = sfswc.derivation_name
        LEFT JOIN current_deployments cd ON lsbs.derivation_name = cd.hostname
    GROUP BY
        lsbs.commit_id
)
SELECT
    ce.flake_id,
    f.name AS flake_name,
    ce.commit_id,
    ce.git_commit_hash,
    LEFT (ce.git_commit_hash,
        8) AS short_hash,
    ce.commit_timestamp,
    ce.total_evaluations,
    ce.successful_evaluations,
    ce.evaluation_statuses,
    ce.evaluated_targets,
    -- "Deployment" timeline (really first-seen timeline)
    cds.first_system_seen AS first_deployment,
    cds.last_system_seen AS last_deployment,
    COALESCE(cds.systems_seen_after_commit, 0) AS total_systems_deployed,
    COALESCE(cds.currently_active_systems, 0) AS currently_deployed_systems,
    cds.systems_seen_list AS deployed_systems,
    cds.currently_active_systems_list AS currently_deployed_systems_list
FROM
    commit_evaluations ce
    JOIN flakes f ON ce.flake_id = f.id
    LEFT JOIN commit_deployment_stats cds ON ce.commit_id = cds.commit_id
ORDER BY
    ce.commit_timestamp DESC;

COMMENT ON VIEW public.view_commit_deployment_timeline IS 'Shows commit evaluation timeline and approximates deployment by tracking when systems were first seen after commit timestamp.';


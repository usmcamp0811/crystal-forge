-- Fix views to use new evaluation status values
-- ============================================================================
-- Drop all views in dependency order (dependents first)
-- Drop all views that depend on other views first
DROP VIEW IF EXISTS view_systems_status_table;

DROP VIEW IF EXISTS view_recent_failed_evaluations;

DROP VIEW IF EXISTS view_evaluation_queue_status;

DROP VIEW IF EXISTS view_systems_cve_summary;

DROP VIEW IF EXISTS view_system_vulnerabilities;

DROP VIEW IF EXISTS view_environment_security_posture;

DROP VIEW IF EXISTS view_critical_vulnerabilities_alert;

-- Drop views that depend on view_systems_current_state
DROP VIEW IF EXISTS view_systems_current_state;

-- Drop views that depend on view_systems_latest_flake_commit
DROP VIEW IF EXISTS view_systems_latest_flake_commit;

-- Drop the base views
DROP VIEW IF EXISTS view_commit_deployment_timeline;

-- Now recreate all views in the correct order
-- 1. First recreate view_commit_deployment_timeline
CREATE OR REPLACE VIEW view_commit_deployment_timeline AS
SELECT
    c.flake_id,
    f.name AS flake_name,
    c.id AS commit_id,
    c.git_commit_hash,
    LEFT (c.git_commit_hash,
        8) AS short_hash,
    c.commit_timestamp,
    MIN(ss.timestamp) AS first_deployment,
    MAX(ss.timestamp) AS last_deployment,
    COUNT(DISTINCT ss.hostname) AS total_systems_deployed,
    COUNT(DISTINCT current_sys.hostname) AS currently_deployed_systems,
    STRING_AGG(DISTINCT ss.hostname, ', ') AS deployed_systems,
    STRING_AGG(DISTINCT CASE WHEN current_sys.hostname IS NOT NULL THEN
            current_sys.hostname
        END, ', ') AS currently_deployed_systems_list
FROM
    commits c
    JOIN flakes f ON c.flake_id = f.id
    JOIN evaluation_targets et ON c.id = et.commit_id
        AND et.status IN ('build-complete', 'complete') -- Support both new and legacy status values
    LEFT JOIN system_states ss ON et.derivation_path = ss.derivation_path
    LEFT JOIN ( SELECT DISTINCT ON (hostname)
            hostname,
            derivation_path
        FROM
            system_states
        ORDER BY
            hostname,
            timestamp DESC) current_sys ON et.derivation_path = current_sys.derivation_path
    AND et.target_name = current_sys.hostname
WHERE
    c.commit_timestamp >= NOW() - INTERVAL '30 days'
GROUP BY
    c.flake_id,
    c.id,
    c.git_commit_hash,
    c.commit_timestamp,
    f.name
ORDER BY
    c.commit_timestamp DESC;

-- 2. Recreate view_systems_latest_flake_commit
CREATE VIEW view_systems_latest_flake_commit AS
SELECT
    latest.system_id AS id,
    latest.hostname,
    latest.commit_id,
    latest.git_commit_hash,
    latest.commit_timestamp,
    latest.repo_url
FROM (
    SELECT
        et.*,
        s.id AS system_id,
        s.hostname,
        f.id AS flake_id,
        c.commit_timestamp,
        c.git_commit_hash,
        f.repo_url,
        ROW_NUMBER() OVER (PARTITION BY s.id, f.id ORDER BY c.commit_timestamp DESC) AS rn
    FROM
        evaluation_targets et
        JOIN systems s ON et.target_name = s.hostname
        JOIN commits c ON c.id = et.commit_id
        JOIN flakes f ON f.id = c.flake_id
    WHERE
        et.status IN ('build-complete', 'complete') -- Support both new and legacy status values
) latest
WHERE
    latest.rn = 1;

-- 3. Recreate view_systems_current_state
CREATE OR REPLACE VIEW view_systems_current_state AS
SELECT
    s.id AS system_id,
    s.hostname,
    -- Current deployment info (from latest flake commit)
    vslfc.repo_url,
    vslfc.git_commit_hash AS deployed_commit_hash,
    vslfc.commit_timestamp AS deployed_commit_timestamp,
    -- System state info (from latest system state)
    lss.derivation_path AS current_derivation_path,
    lss.primary_ip_address AS ip_address,
    ROUND(lss.uptime_secs::numeric / 86400, 1) AS uptime_days,
    lss.os,
    lss.kernel,
    lss.agent_version,
    lss.timestamp AS last_deployed,
    -- Latest commit evaluation info
    latest_eval.derivation_path AS latest_commit_derivation_path,
    latest_eval.evaluation_status AS latest_commit_evaluation_status,
    -- Comparison: is system running the latest evaluated derivation?
    CASE WHEN lss.derivation_path IS NOT NULL
        AND latest_eval.derivation_path IS NOT NULL
        AND lss.derivation_path = latest_eval.derivation_path THEN
        TRUE
    WHEN latest_eval.derivation_path IS NULL THEN
        NULL -- No evaluation exists for latest commit
    ELSE
        FALSE
    END AS is_running_latest_derivation,
    -- Heartbeat info
    lhb.last_heartbeat,
    GREATEST (COALESCE(lss.timestamp, '1970-01-01'::timestamp), COALESCE(lhb.last_heartbeat, '1970-01-01'::timestamp)) AS last_seen
FROM
    systems s -- Start with ALL systems as the base
    LEFT JOIN view_systems_latest_flake_commit vslfc ON s.hostname = vslfc.hostname
    LEFT JOIN (
        -- Get latest system state per hostname
        SELECT DISTINCT ON (hostname)
            hostname,
            derivation_path,
            primary_ip_address,
            uptime_secs,
            os,
            kernel,
            agent_version,
            timestamp
        FROM
            system_states
        ORDER BY
            hostname,
            timestamp DESC) lss ON s.hostname = lss.hostname
    LEFT JOIN (
        -- Get latest heartbeat per hostname
        SELECT DISTINCT ON (ss.hostname)
            ss.hostname,
            ah.timestamp AS last_heartbeat
        FROM
            system_states ss
            JOIN agent_heartbeats ah ON ah.system_state_id = ss.id
        ORDER BY
            ss.hostname,
            ah.timestamp DESC) lhb ON s.hostname = lhb.hostname
    LEFT JOIN (
        -- Get evaluation for the latest commit for each system
        SELECT
            et.target_name AS hostname,
            et.derivation_path,
            et.status AS evaluation_status
        FROM
            evaluation_targets et
            JOIN view_systems_latest_flake_commit vlc ON et.commit_id = vlc.commit_id
                AND et.target_name = vlc.hostname
        WHERE
            et.status IN ('build-complete', 'complete') -- Support both new and legacy status values
) latest_eval ON s.hostname = latest_eval.hostname
ORDER BY
    s.hostname;

-- 4. Recreate view_systems_status_table
CREATE VIEW view_systems_status_table AS
WITH system_derivation_source AS (
    -- Find if current derivation path maps to any known evaluation target
    SELECT
        sys.hostname,
        sys.current_derivation_path,
        sys.latest_commit_derivation_path,
        sys.latest_commit_evaluation_status,
        sys.last_seen,
        sys.agent_version,
        sys.uptime_days,
        sys.ip_address,
        sys.last_deployed,
        -- Check if current derivation matches any completed evaluation
        CASE WHEN EXISTS (
            SELECT
                1
            FROM
                evaluation_targets et
            WHERE
                et.derivation_path = sys.current_derivation_path
                AND et.status IN ('build-complete', 'complete') -- Support both new and legacy status values
) THEN
            TRUE
        ELSE
            FALSE
        END AS derivation_is_known,
        -- Get the most recent commit timestamp for this system's flake
        latest_commit.commit_timestamp AS latest_flake_commit_timestamp,
        -- Check if there are pending/in-progress evaluations for newer commits
        CASE WHEN EXISTS (
            SELECT
                1
            FROM
                evaluation_targets et
                JOIN commits c ON et.commit_id = c.id
                JOIN flakes f ON c.flake_id = f.id
                JOIN systems s ON s.hostname = sys.hostname
            WHERE
                s.flake_id = f.id
                AND et.target_name = sys.hostname
                AND et.status IN ('dry-run-pending', 'dry-run-inprogress', 'build-pending', 'build-inprogress', 'pending', 'queued', 'in-progress') -- Support both new and legacy status values
                AND c.commit_timestamp > sys.last_deployed) THEN
            TRUE
        ELSE
            FALSE
        END AS has_pending_newer_evaluation,
        -- Get timestamp of oldest pending evaluation (to check for stuck evaluations)
        (
            SELECT
                MIN(et.scheduled_at)
            FROM evaluation_targets et
            JOIN commits c ON et.commit_id = c.id
            JOIN flakes f ON c.flake_id = f.id
            JOIN systems s ON s.hostname = sys.hostname
        WHERE
            s.flake_id = f.id
            AND et.target_name = sys.hostname
            AND et.status IN ('dry-run-pending', 'dry-run-inprogress', 'build-pending', 'build-inprogress', 'pending', 'queued', 'in-progress') -- Support both new and legacy status values
) AS oldest_pending_eval_time
    FROM
        view_systems_current_state sys
        LEFT JOIN (
            -- Get latest commit timestamp for each system's flake
            SELECT DISTINCT ON (s.hostname)
                s.hostname,
                c.commit_timestamp
            FROM
                systems s
                JOIN flakes f ON s.flake_id = f.id
                JOIN commits c ON f.id = c.flake_id
            ORDER BY
                s.hostname,
                c.commit_timestamp DESC) latest_commit ON sys.hostname = latest_commit.hostname
)
    SELECT
        hostname,
        -- Determine status symbol based on the new logic
        CASE
        -- No system state at all
        WHEN current_derivation_path IS NULL THEN
            '⚫' -- No System State
            -- Current derivation is known (maps to completed evaluation)
        WHEN derivation_is_known = TRUE THEN
            CASE
            -- Running latest evaluated derivation
            WHEN current_derivation_path = latest_commit_derivation_path THEN
                '🟢' -- Current
                -- Running older known derivation, but evaluation pending for newer commit
            WHEN has_pending_newer_evaluation = TRUE THEN
                CASE
                -- Evaluation has been pending too long (>1 hour), mark as unknown
                WHEN oldest_pending_eval_time < NOW() - INTERVAL '1 hour' THEN
                    '🟤' -- Unknown (stuck evaluation)
                    -- Normal evaluation in progress
                ELSE
                    '🟡' -- Evaluating
                END
                -- Running older known derivation, no newer commits to evaluate
            ELSE
                '🔴' -- Behind
            END
            -- Current derivation is unknown (doesn't map to any completed evaluation)
        WHEN derivation_is_known = FALSE THEN
            CASE
            -- There was a new commit since last deployment, might be evaluating
            WHEN latest_flake_commit_timestamp > last_deployed
                AND has_pending_newer_evaluation = TRUE THEN
                CASE
                -- Evaluation has been pending too long (>1 hour)
                WHEN oldest_pending_eval_time < NOW() - INTERVAL '1 hour' THEN
                    '🟤' -- Unknown (stuck evaluation)
                    -- Normal evaluation in progress
                ELSE
                    '🟡' -- Evaluating
                END
                -- No new commits or no pending evaluations - truly unknown
            ELSE
                '🟤' -- Unknown
            END
            -- Fallback
        ELSE
            '⚪' -- No Evaluation Data
        END AS status,
        -- Add text status column for Grafana pie charts
        CASE
        -- No system state at all
        WHEN current_derivation_path IS NULL THEN
            'Offline'
            -- Current derivation is known (maps to completed evaluation)
        WHEN derivation_is_known = TRUE THEN
            CASE
            -- Running latest evaluated derivation
            WHEN current_derivation_path = latest_commit_derivation_path THEN
                'Up to Date'
                -- Running older known derivation, but evaluation pending for newer commit
            WHEN has_pending_newer_evaluation = TRUE THEN
                CASE
                -- Evaluation has been pending too long (>1 hour), mark as unknown
                WHEN oldest_pending_eval_time < NOW() - INTERVAL '1 hour' THEN
                    'Evaluation Stuck'
                    -- Normal evaluation in progress
                ELSE
                    'Updating'
                END
                -- Running older known derivation, no newer commits to evaluate
            ELSE
                'Outdated'
            END
            -- Current derivation is unknown (doesn't map to any completed evaluation)
        WHEN derivation_is_known = FALSE THEN
            CASE
            -- There was a new commit since last deployment, might be evaluating
            WHEN latest_flake_commit_timestamp > last_deployed
                AND has_pending_newer_evaluation = TRUE THEN
                CASE
                -- Evaluation has been pending too long (>1 hour)
                WHEN oldest_pending_eval_time < NOW() - INTERVAL '1 hour' THEN
                    'Evaluation Stuck'
                    -- Normal evaluation in progress
                ELSE
                    'Updating'
                END
                -- No new commits or no pending evaluations - truly unknown
            ELSE
                'Unknown State'
            END
            -- Fallback
        ELSE
            'No Data'
        END AS status_text,
        CONCAT(EXTRACT(EPOCH FROM NOW() - last_seen)::int / 60, 'm ago') AS last_seen,
    agent_version AS version,
    uptime_days || 'd' AS uptime,
    ip_address
FROM
    system_derivation_source
ORDER BY
    CASE
    -- Current derivation is known and is latest
    WHEN derivation_is_known = TRUE
        AND current_derivation_path = latest_commit_derivation_path THEN
        1 -- 🟢 Current
        -- Evaluating (either known or unknown derivation with pending newer evaluations)
    WHEN (derivation_is_known = TRUE
        AND has_pending_newer_evaluation = TRUE
        AND oldest_pending_eval_time >= NOW() - INTERVAL '1 hour')
        OR (derivation_is_known = FALSE
            AND latest_flake_commit_timestamp > last_deployed
            AND has_pending_newer_evaluation = TRUE
            AND oldest_pending_eval_time >= NOW() - INTERVAL '1 hour') THEN
        2 -- 🟡 Evaluating
        -- Behind (known derivation, older than latest, no pending evaluations)
    WHEN derivation_is_known = TRUE
        AND current_derivation_path != latest_commit_derivation_path
        AND has_pending_newer_evaluation = FALSE THEN
        3 -- 🔴 Behind
        -- Unknown (various unknown states or stuck evaluations)
    WHEN derivation_is_known = FALSE
        OR oldest_pending_eval_time < NOW() - INTERVAL '1 hour' THEN
        4 -- 🟤 Unknown
        -- No evaluation data
    WHEN latest_commit_derivation_path IS NULL THEN
        5 -- ⚪ No Evaluation
        -- No system state
    WHEN current_derivation_path IS NULL THEN
        6 -- ⚫ No System State
    ELSE
        7
    END,
    last_seen DESC;

-- 5. Recreate view_recent_failed_evaluations
CREATE OR REPLACE VIEW view_recent_failed_evaluations AS
SELECT
    et.target_name AS hostname,
    et.status,
    et.started_at,
    et.completed_at,
    et.evaluation_duration_ms,
    et.attempt_count,
    c.git_commit_hash,
    c.commit_timestamp,
    f.name AS flake_name,
    f.repo_url,
    -- Time since failure
    EXTRACT(EPOCH FROM (NOW() - et.completed_at)) / 3600 AS hours_since_failure
FROM
    evaluation_targets et
    JOIN commits c ON et.commit_id = c.id
    JOIN flakes f ON c.flake_id = f.id
WHERE
    et.status IN ('dry-run-failed', 'build-failed', 'failed') -- Support both new and legacy status values
    AND et.completed_at >= NOW() - INTERVAL '7 days' -- Last 7 days
ORDER BY
    et.completed_at DESC;

-- 6. Recreate view_evaluation_queue_status
CREATE VIEW view_evaluation_queue_status AS
SELECT
    target_name,
    status,
    scheduled_at,
    started_at,
    evaluation_duration_ms AS duration_ms,
    attempt_count
FROM
    evaluation_targets
WHERE
    status IN ('dry-run-pending', 'dry-run-inprogress', 'build-pending', 'build-inprogress', 'pending', 'queued', 'in-progress') -- Support both new and legacy status values
    OR (status IN ('dry-run-complete', 'build-complete', 'dry-run-failed', 'build-failed', 'complete', 'failed') -- Support both new and legacy status values
        AND completed_at > NOW() - INTERVAL '24 hours')
ORDER BY
    CASE status
    WHEN 'build-inprogress' THEN
        1
    WHEN 'in-progress' THEN
        1
    WHEN 'dry-run-inprogress' THEN
        2
    WHEN 'build-pending' THEN
        3
    WHEN 'queued' THEN
        3
    WHEN 'dry-run-pending' THEN
        4
    WHEN 'pending' THEN
        4
    WHEN 'build-failed' THEN
        5
    WHEN 'dry-run-failed' THEN
        5
    WHEN 'failed' THEN
        5
    WHEN 'build-complete' THEN
        6
    WHEN 'dry-run-complete' THEN
        6
    WHEN 'complete' THEN
        6
    END,
    scheduled_at;

COMMENT ON VIEW view_evaluation_queue_status IS 'Current evaluation pipeline status - active evaluations and recent completions (updated for new status values)';

-- 7. Recreate CVE-related views
CREATE OR REPLACE VIEW view_systems_cve_summary AS
SELECT
    sys.hostname,
    sys.current_derivation_path,
    sys.last_deployed,
    sys.last_seen,
    sys.ip_address,
    latest_scan.completed_at AS last_cve_scan,
    latest_scan.scanner_name,
    COALESCE(latest_scan.total_packages, 0) AS total_packages,
    COALESCE(latest_scan.total_vulnerabilities, 0) AS total_cves,
    COALESCE(latest_scan.critical_count, 0) AS critical_cves,
    COALESCE(latest_scan.high_count, 0) AS high_cves,
    COALESCE(latest_scan.medium_count, 0) AS medium_cves,
    COALESCE(latest_scan.low_count, 0) AS low_cves,
    CASE WHEN latest_scan.total_vulnerabilities = 0 THEN
        'Clean'
    WHEN latest_scan.critical_count > 0 THEN
        'Critical'
    WHEN latest_scan.high_count > 0 THEN
        'High Risk'
    WHEN latest_scan.medium_count > 0 THEN
        'Medium Risk'
    WHEN latest_scan.low_count > 0 THEN
        'Low Risk'
    ELSE
        'Unknown'
    END AS security_status,
    CASE WHEN latest_scan.completed_at IS NOT NULL THEN
        EXTRACT(EPOCH FROM (NOW() - latest_scan.completed_at)) / 86400
    ELSE
        NULL
    END AS days_since_scan
FROM
    view_systems_current_state sys
    LEFT JOIN ( SELECT DISTINCT ON (et.target_name)
            et.target_name AS hostname,
            cs.completed_at,
            cs.scanner_name,
            cs.total_packages,
            cs.total_vulnerabilities,
            cs.critical_count,
            cs.high_count,
            cs.medium_count,
            cs.low_count
        FROM
            evaluation_targets et
            JOIN cve_scans cs ON et.id = cs.evaluation_target_id
        WHERE
            et.target_type = 'nixos'
            AND et.status IN ('build-complete', 'complete') -- Support both new and legacy status values
            AND cs.completed_at IS NOT NULL
        ORDER BY
            et.target_name,
            cs.completed_at DESC) latest_scan ON sys.hostname = latest_scan.hostname
ORDER BY
    sys.hostname;

CREATE OR REPLACE VIEW view_system_vulnerabilities AS
SELECT
    et.target_name AS hostname,
    np.name AS package_name,
    np.pname AS package_pname,
    np.version AS package_version,
    np.derivation_path,
    c.id AS cve_id,
    c.cvss_v3_score,
    severity_from_cvss (c.cvss_v3_score) AS severity,
    c.description,
    pv.is_whitelisted,
    pv.whitelist_reason,
    pv.fixed_version,
    pv.detection_method,
    scan.completed_at,
    scan.scanner_name,
    et.derivation_path AS evaluation_derivation_path,
    commits.git_commit_hash,
    flakes.name AS flake_name
FROM
    evaluation_targets et
    JOIN commits ON et.commit_id = commits.id
    JOIN flakes ON commits.flake_id = flakes.id
    JOIN cve_scans scan ON et.id = scan.evaluation_target_id
    JOIN scan_packages sp ON scan.id = sp.scan_id
    JOIN nix_packages np ON sp.derivation_path = np.derivation_path
    JOIN package_vulnerabilities pv ON np.derivation_path = pv.derivation_path
    JOIN cves c ON pv.cve_id = c.id
WHERE
    et.target_type = 'nixos'
    AND et.status IN ('build-complete', 'complete') -- Support both new and legacy status values
    AND scan.completed_at IS NOT NULL
    AND NOT pv.is_whitelisted
ORDER BY
    et.target_name,
    c.cvss_v3_score DESC NULLS LAST;

CREATE OR REPLACE VIEW view_environment_security_posture AS
SELECT
    e.name AS environment,
    e.compliance_level_id,
    cl.name AS compliance_level,
    COUNT(DISTINCT et.target_name) AS total_systems,
    COUNT(DISTINCT CASE WHEN latest_scan.total_vulnerabilities = 0 THEN
            et.target_name
        END) AS clean_systems,
    COUNT(DISTINCT CASE WHEN latest_scan.critical_count > 0 THEN
            et.target_name
        END) AS critical_systems,
    COUNT(DISTINCT CASE WHEN latest_scan.high_count > 0 THEN
            et.target_name
        END) AS high_risk_systems,
    AVG(latest_scan.total_vulnerabilities) AS avg_vulnerabilities_per_system,
    MIN(latest_scan.completed_at) AS oldest_scan,
    MAX(latest_scan.completed_at) AS newest_scan
FROM
    environments e
    LEFT JOIN systems s ON e.id = s.environment_id
        AND s.is_active = TRUE
    LEFT JOIN compliance_levels cl ON e.compliance_level_id = cl.id
    LEFT JOIN evaluation_targets et ON s.hostname = et.target_name
        AND et.target_type = 'nixos'
        AND et.status IN ('build-complete', 'complete') -- Support both new and legacy status values
    LEFT JOIN ( SELECT DISTINCT ON (et.id)
            et.id,
            et.target_name,
            cs.completed_at,
            cs.total_vulnerabilities,
            cs.critical_count,
            cs.high_count
        FROM
            evaluation_targets et
            JOIN cve_scans cs ON et.id = cs.evaluation_target_id
        WHERE
            et.target_type = 'nixos'
            AND et.status IN ('build-complete', 'complete') -- Support both new and legacy status values
            AND cs.completed_at IS NOT NULL
        ORDER BY
            et.id,
            cs.completed_at DESC) latest_scan ON et.id = latest_scan.id
GROUP BY
    e.id,
    e.name,
    e.compliance_level_id,
    cl.name
ORDER BY
    e.name;

CREATE OR REPLACE VIEW view_critical_vulnerabilities_alert AS
SELECT
    et.target_name AS hostname,
    sys_state.primary_ip_address AS ip_address,
    e.name AS environment,
    cl.name AS compliance_level,
    COUNT(DISTINCT c.id) AS critical_cve_count,
    STRING_AGG(DISTINCT c.id, ', ' ORDER BY c.id) AS critical_cves,
    MAX(c.cvss_v3_score) AS highest_cvss_score,
    latest_scan.completed_at AS last_scan_date,
    et.derivation_path AS evaluation_derivation_path,
    commits.git_commit_hash,
    flakes.name AS flake_name
FROM
    evaluation_targets et
    JOIN commits ON et.commit_id = commits.id
    JOIN flakes ON commits.flake_id = flakes.id
    LEFT JOIN systems s ON et.target_name = s.hostname
    LEFT JOIN environments e ON s.environment_id = e.id
    LEFT JOIN compliance_levels cl ON e.compliance_level_id = cl.id
    LEFT JOIN (
        -- Get latest system state for IP address
        SELECT DISTINCT ON (hostname)
            hostname,
            primary_ip_address
        FROM
            system_states
        ORDER BY
            hostname,
            timestamp DESC) sys_state ON et.target_name = sys_state.hostname
    JOIN ( SELECT DISTINCT ON (cs.evaluation_target_id)
            cs.evaluation_target_id,
            cs.id,
            cs.completed_at
        FROM
            cve_scans cs
        WHERE
            cs.completed_at IS NOT NULL
        ORDER BY
            cs.evaluation_target_id,
            cs.completed_at DESC) latest_scan ON et.id = latest_scan.evaluation_target_id
    JOIN scan_packages sp ON latest_scan.id = sp.scan_id
    JOIN package_vulnerabilities pv ON sp.derivation_path = pv.derivation_path
    JOIN cves c ON pv.cve_id = c.id
WHERE
    et.target_type = 'nixos'
    AND et.status IN ('build-complete', 'complete') -- Support both new and legacy status values
    AND severity_from_cvss (c.cvss_v3_score) = 'CRITICAL'
    AND NOT pv.is_whitelisted
    AND (pv.whitelist_expires_at IS NULL
        OR pv.whitelist_expires_at < NOW())
GROUP BY
    et.target_name,
    sys_state.primary_ip_address,
    e.name,
    cl.name,
    latest_scan.completed_at,
    et.derivation_path,
    commits.git_commit_hash,
    flakes.name
HAVING
    COUNT(DISTINCT c.id) > 0
ORDER BY
    critical_cve_count DESC,
    highest_cvss_score DESC;


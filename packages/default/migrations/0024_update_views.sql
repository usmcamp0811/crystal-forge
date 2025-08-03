-- Update view_commit_deployment_timeline
CREATE OR REPLACE VIEW public.view_commit_deployment_timeline AS
SELECT
    c.flake_id,
    f.name AS flake_name,
    c.id AS commit_id,
    c.git_commit_hash,
    "left" (c.git_commit_hash, 8) AS short_hash,
    c.commit_timestamp,
    count(DISTINCT d.id) AS total_evaluations,
    count(DISTINCT d.id) FILTER (WHERE (d.derivation_path IS NOT NULL)) AS successful_evaluations,
string_agg(DISTINCT d.status, ', '::text) AS evaluation_statuses,
string_agg(DISTINCT d.derivation_name, ', '::text) AS evaluated_targets,
min(ss."timestamp") AS first_deployment,
max(ss."timestamp") AS last_deployment,
count(DISTINCT ss.hostname) AS total_systems_deployed,
count(DISTINCT current_sys.hostname) AS currently_deployed_systems,
string_agg(DISTINCT ss.hostname, ', '::text) AS deployed_systems,
string_agg(DISTINCT CASE WHEN (current_sys.hostname IS NOT NULL) THEN
        current_sys.hostname
    ELSE
        NULL::text
    END, ', '::text) AS currently_deployed_systems_list
FROM ((((public.commits c
                JOIN public.flakes f ON (c.flake_id = f.id))
            LEFT JOIN public.derivations d ON (((c.id = d.commit_id)
                        AND (d.derivation_type = 'nixos'::text)
                        AND (d.status = ANY (ARRAY['build-complete'::text, 'complete'::text])))))
        LEFT JOIN public.system_states ss ON (((d.derivation_path = ss.derivation_path)
                    AND (d.derivation_path IS NOT NULL))))
    LEFT JOIN ( SELECT DISTINCT ON (system_states.hostname)
            system_states.hostname,
            system_states.derivation_path
        FROM
            public.system_states
        ORDER BY
            system_states.hostname,
            system_states."timestamp" DESC) current_sys ON (((d.derivation_path = current_sys.derivation_path)
                AND (d.derivation_name = current_sys.hostname)
                AND (d.derivation_path IS NOT NULL))))
WHERE (c.commit_timestamp >= (now() - '30 days'::interval))
GROUP BY
    c.flake_id,
    c.id,
    c.git_commit_hash,
    c.commit_timestamp,
    f.name
ORDER BY
    c.commit_timestamp DESC;

-- Update view_critical_vulnerabilities_alert
CREATE OR REPLACE VIEW public.view_critical_vulnerabilities_alert AS
SELECT
    d.derivation_name AS hostname,
    sys_state.primary_ip_address AS ip_address,
    e.name AS environment,
    cl.name AS compliance_level,
    count(DISTINCT c.id) AS critical_cve_count,
    string_agg(DISTINCT (c.id)::text, ', '::text) AS critical_cves,
    max(c.cvss_v3_score) AS highest_cvss_score,
    latest_scan.completed_at AS last_scan_date,
    d.derivation_path AS evaluation_derivation_path,
    commits.git_commit_hash,
    flakes.name AS flake_name
FROM ((((((((((public.derivations d
                                        JOIN public.commits ON (d.commit_id = commits.id))
                                    JOIN public.flakes ON (commits.flake_id = flakes.id))
                                LEFT JOIN public.systems s ON (d.derivation_name = s.hostname))
                            LEFT JOIN public.environments e ON (s.environment_id = e.id))
                        LEFT JOIN public.compliance_levels cl ON (e.compliance_level_id = cl.id))
                    LEFT JOIN ( SELECT DISTINCT ON (system_states.hostname)
                            system_states.hostname,
                            system_states.primary_ip_address
                        FROM
                            public.system_states
                        ORDER BY
                            system_states.hostname,
                            system_states."timestamp" DESC) sys_state ON (d.derivation_name = sys_state.hostname))
                    JOIN ( SELECT DISTINCT ON (cs.derivation_id)
                            cs.derivation_id,
                            cs.id,
                            cs.completed_at
                        FROM
                            public.cve_scans cs
                        WHERE (cs.completed_at IS NOT NULL)
                    ORDER BY
                        cs.derivation_id,
                        cs.completed_at DESC) latest_scan ON (d.id = latest_scan.derivation_id))
                JOIN public.scan_packages sp ON (latest_scan.id = sp.scan_id))
            JOIN public.package_vulnerabilities pv ON (sp.derivation_id = pv.derivation_id))
        JOIN public.cves c ON (((pv.cve_id)::text = (c.id)::text)))
WHERE ((d.derivation_type = 'nixos'::text)
    AND (d.status = ANY (ARRAY['build-complete'::text, 'complete'::text]))
    AND ((public.severity_from_cvss (c.cvss_v3_score))::text = 'CRITICAL'::text)
    AND (NOT pv.is_whitelisted)
    AND ((pv.whitelist_expires_at IS NULL)
        OR (pv.whitelist_expires_at < now())))
GROUP BY
    d.derivation_name,
    sys_state.primary_ip_address,
    e.name,
    cl.name,
    latest_scan.completed_at,
    d.derivation_path,
    commits.git_commit_hash,
    flakes.name
HAVING (count(DISTINCT c.id) > 0)
ORDER BY
    (count(DISTINCT c.id)) DESC,
    (max(c.cvss_v3_score)) DESC;

-- Update view_environment_security_posture
CREATE OR REPLACE VIEW public.view_environment_security_posture AS
SELECT
    e.name AS environment,
    e.compliance_level_id,
    cl.name AS compliance_level,
    count(DISTINCT d.derivation_name) AS total_systems,
    count(DISTINCT CASE WHEN (latest_scan.total_vulnerabilities = 0) THEN
            d.derivation_name
        ELSE
            NULL::text
        END) AS clean_systems,
    count(DISTINCT CASE WHEN (latest_scan.critical_count > 0) THEN
            d.derivation_name
        ELSE
            NULL::text
        END) AS critical_systems,
    count(DISTINCT CASE WHEN (latest_scan.high_count > 0) THEN
            d.derivation_name
        ELSE
            NULL::text
        END) AS high_risk_systems,
    avg(latest_scan.total_vulnerabilities) AS avg_vulnerabilities_per_system,
    min(latest_scan.completed_at) AS oldest_scan,
    max(latest_scan.completed_at) AS newest_scan
FROM ((((public.environments e
            LEFT JOIN public.systems s ON (((e.id = s.environment_id)
                        AND (s.is_active = TRUE))))
        LEFT JOIN public.compliance_levels cl ON (e.compliance_level_id = cl.id))
    LEFT JOIN public.derivations d ON (((s.hostname = d.derivation_name)
                AND (d.derivation_type = 'nixos'::text)
                AND (d.status = ANY (ARRAY['build-complete'::text, 'complete'::text])))))
    LEFT JOIN ( SELECT DISTINCT ON (d_1.id)
            d_1.id,
            d_1.derivation_name,
            cs.completed_at,
            cs.total_vulnerabilities,
            cs.critical_count,
            cs.high_count
        FROM (public.derivations d_1
            JOIN public.cve_scans cs ON (d_1.id = cs.derivation_id))
    WHERE ((d_1.derivation_type = 'nixos'::text)
        AND (d_1.status = ANY (ARRAY['build-complete'::text, 'complete'::text]))
        AND (cs.completed_at IS NOT NULL))
ORDER BY
    d_1.id,
    cs.completed_at DESC) latest_scan ON (d.id = latest_scan.id))
GROUP BY
    e.id,
    e.name,
    e.compliance_level_id,
    cl.name
ORDER BY
    e.name;

-- Update view_evaluation_pipeline_debug
CREATE OR REPLACE VIEW public.view_evaluation_pipeline_debug AS
SELECT
    f.name AS flake_name,
    c.git_commit_hash,
    "left" (c.git_commit_hash, 8) AS short_hash,
    c.commit_timestamp,
    d.derivation_name AS target_name,
    d.derivation_type AS target_type,
    d.status,
    d.scheduled_at,
    d.started_at,
    d.completed_at,
    CASE WHEN (d.derivation_path IS NOT NULL) THEN
        'HAS_DERIVATION'::text
    ELSE
        'NO_DERIVATION'::text
    END AS has_derivation,
    d.attempt_count,
    d.error_message
FROM ((public.commits c
        JOIN public.flakes f ON (c.flake_id = f.id))
    LEFT JOIN public.derivations d ON (c.id = d.commit_id))
WHERE (c.commit_timestamp >= (now() - '7 days'::interval))
ORDER BY
    c.commit_timestamp DESC,
    d.derivation_name;

-- Update view_evaluation_queue_status
CREATE OR REPLACE VIEW public.view_evaluation_queue_status AS
SELECT
    derivation_name AS target_name,
    status,
    scheduled_at,
    started_at,
    evaluation_duration_ms AS duration_ms,
    attempt_count
FROM
    public.derivations
WHERE ((derivation_type = 'nixos'::text)
    AND ((status = ANY (ARRAY['dry-run-pending'::text, 'dry-run-inprogress'::text, 'build-pending'::text, 'build-inprogress'::text, 'pending'::text, 'queued'::text, 'in-progress'::text]))
        OR ((status = ANY (ARRAY['dry-run-complete'::text, 'build-complete'::text, 'dry-run-failed'::text, 'build-failed'::text, 'complete'::text, 'failed'::text]))
            AND (completed_at > (now() - '24:00:00'::interval)))))
ORDER BY
    CASE status
    WHEN 'build-inprogress'::text THEN
        1
    WHEN 'in-progress'::text THEN
        1
    WHEN 'dry-run-inprogress'::text THEN
        2
    WHEN 'build-pending'::text THEN
        3
    WHEN 'queued'::text THEN
        3
    WHEN 'dry-run-pending'::text THEN
        4
    WHEN 'pending'::text THEN
        4
    WHEN 'build-failed'::text THEN
        5
    WHEN 'dry-run-failed'::text THEN
        5
    WHEN 'failed'::text THEN
        5
    WHEN 'build-complete'::text THEN
        6
    WHEN 'dry-run-complete'::text THEN
        6
    WHEN 'complete'::text THEN
        6
    ELSE
        NULL::integer
    END,
    scheduled_at;

-- Update view_recent_failed_evaluations
CREATE OR REPLACE VIEW public.view_recent_failed_evaluations AS
SELECT
    d.derivation_name AS hostname,
    d.status,
    d.started_at,
    d.completed_at,
    d.evaluation_duration_ms,
    d.attempt_count,
    c.git_commit_hash,
    c.commit_timestamp,
    f.name AS flake_name,
    f.repo_url,
    (EXTRACT(epoch FROM (now() - d.completed_at)) / (3600)::numeric) AS hours_since_failure
FROM ((public.derivations d
        JOIN public.commits c ON (d.commit_id = c.id))
    JOIN public.flakes f ON (c.flake_id = f.id))
WHERE ((d.derivation_type = 'nixos'::text)
    AND (d.status = ANY (ARRAY['dry-run-failed'::text, 'build-failed'::text, 'failed'::text]))
    AND (d.completed_at >= (now() - '7 days'::interval)))
ORDER BY
    d.completed_at DESC;

-- Update view_systems_latest_flake_commit
CREATE OR REPLACE VIEW public.view_systems_latest_flake_commit AS
SELECT
    system_id AS id,
    hostname,
    commit_id,
    git_commit_hash,
    commit_timestamp,
    repo_url
FROM (
    SELECT
        d.id,
        d.commit_id,
        d.derivation_type,
        d.derivation_name,
        d.derivation_path,
        d.scheduled_at,
        d.completed_at,
        d.attempt_count,
        d.status,
        d.started_at,
        d.evaluation_duration_ms,
        d.error_message,
        s.id AS system_id,
        s.hostname,
        f.id AS flake_id,
        c.commit_timestamp,
        c.git_commit_hash,
        f.repo_url,
        row_number() OVER (PARTITION BY s.id, f.id ORDER BY c.commit_timestamp DESC) AS rn
    FROM (((public.derivations d
                JOIN public.systems s ON (d.derivation_name = s.hostname))
            JOIN public.commits c ON (c.id = d.commit_id))
        JOIN public.flakes f ON (f.id = c.flake_id))
WHERE ((d.derivation_type = 'nixos'::text)
    AND (d.status = ANY (ARRAY['build-complete'::text, 'complete'::text])))) latest
WHERE (rn = 1);

-- Update view_system_vulnerabilities
CREATE OR REPLACE VIEW public.view_system_vulnerabilities AS
SELECT
    d.derivation_name AS hostname,
    pkg_d.package_name,
    pkg_d.package_pname AS package_pname,
    pkg_d.package_version AS package_version,
    pkg_d.derivation_path,
    c.id AS cve_id,
    c.cvss_v3_score,
    public.severity_from_cvss (c.cvss_v3_score) AS severity,
    c.description,
    pv.is_whitelisted,
    pv.whitelist_reason,
    pv.fixed_version,
    pv.detection_method,
    scan.completed_at,
    scan.scanner_name,
    d.derivation_path AS evaluation_derivation_path,
    commits.git_commit_hash,
    flakes.name AS flake_name
FROM (((((((public.derivations d
                            JOIN public.commits ON (d.commit_id = commits.id))
                        JOIN public.flakes ON (commits.flake_id = flakes.id))
                    JOIN public.cve_scans scan ON (d.id = scan.derivation_id))
                JOIN public.scan_packages sp ON (scan.id = sp.scan_id))
            JOIN public.derivations pkg_d ON (sp.derivation_id = pkg_d.id))
        JOIN public.package_vulnerabilities pv ON (pkg_d.id = pv.derivation_id))
    JOIN public.cves c ON (((pv.cve_id)::text = (c.id)::text)))
WHERE ((d.derivation_type = 'nixos'::text)
    AND (d.status = ANY (ARRAY['build-complete'::text, 'complete'::text]))
    AND (pkg_d.derivation_type = 'package'::text)
    AND (scan.completed_at IS NOT NULL)
    AND (NOT pv.is_whitelisted))
ORDER BY
    d.derivation_name,
    c.cvss_v3_score DESC NULLS LAST;

-- Update view_systems_current_state
CREATE OR REPLACE VIEW public.view_systems_current_state AS
SELECT
    s.id AS system_id,
    s.hostname,
    vslfc.repo_url,
    vslfc.git_commit_hash AS deployed_commit_hash,
    vslfc.commit_timestamp AS deployed_commit_timestamp,
    lss.derivation_path AS current_derivation_path,
    lss.primary_ip_address AS ip_address,
    round(((lss.uptime_secs)::numeric / (86400)::numeric), 1) AS uptime_days,
    lss.os,
    lss.kernel,
    lss.agent_version,
    lss."timestamp" AS last_deployed,
    latest_eval.derivation_path AS latest_commit_derivation_path,
    latest_eval.evaluation_status AS latest_commit_evaluation_status,
    CASE WHEN ((lss.derivation_path IS NOT NULL)
        AND (latest_eval.derivation_path IS NOT NULL)
        AND (lss.derivation_path = latest_eval.derivation_path)) THEN
        TRUE
    WHEN (latest_eval.derivation_path IS NULL) THEN
        NULL::boolean
    ELSE
        FALSE
    END AS is_running_latest_derivation,
    lhb.last_heartbeat,
    GREATEST (COALESCE(lss."timestamp", ('1970-01-01 00:00:00'::timestamp WITHOUT time zone)::timestamp with time zone), COALESCE(lhb.last_heartbeat, ('1970-01-01 00:00:00'::timestamp WITHOUT time zone)::timestamp with time zone)) AS last_seen
FROM ((((public.systems s
            LEFT JOIN public.view_systems_latest_flake_commit vslfc ON (s.hostname = vslfc.hostname))
        LEFT JOIN ( SELECT DISTINCT ON (system_states.hostname)
                system_states.hostname,
                system_states.derivation_path,
                system_states.primary_ip_address,
                system_states.uptime_secs,
                system_states.os,
                system_states.kernel,
                system_states.agent_version,
                system_states."timestamp"
            FROM
                public.system_states
            ORDER BY
                system_states.hostname,
                system_states."timestamp" DESC) lss ON (s.hostname = lss.hostname))
        LEFT JOIN ( SELECT DISTINCT ON (ss.hostname)
                ss.hostname,
                ah."timestamp" AS last_heartbeat
            FROM (public.system_states ss
                JOIN public.agent_heartbeats ah ON (ah.system_state_id = ss.id))
        ORDER BY
            ss.hostname,
            ah."timestamp" DESC) lhb ON (s.hostname = lhb.hostname))
    LEFT JOIN (
        SELECT
            d.derivation_name AS hostname,
            d.derivation_path,
            d.status AS evaluation_status
        FROM (public.derivations d
            JOIN public.view_systems_latest_flake_commit vlc ON (((d.commit_id = vlc.commit_id)
                        AND (d.derivation_name = vlc.hostname))))
    WHERE ((d.derivation_type = 'nixos'::text)
        AND (d.status = ANY (ARRAY['build-complete'::text, 'complete'::text])))) latest_eval ON (s.hostname = latest_eval.hostname))
ORDER BY
    s.hostname;

-- Update view_systems_cve_summary
CREATE OR REPLACE VIEW public.view_systems_cve_summary AS
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
    CASE WHEN (latest_scan.total_vulnerabilities = 0) THEN
        'Clean'::text
    WHEN (latest_scan.critical_count > 0) THEN
        'Critical'::text
    WHEN (latest_scan.high_count > 0) THEN
        'High Risk'::text
    WHEN (latest_scan.medium_count > 0) THEN
        'Medium Risk'::text
    WHEN (latest_scan.low_count > 0) THEN
        'Low Risk'::text
    ELSE
        'Unknown'::text
    END AS security_status,
    CASE WHEN (latest_scan.completed_at IS NOT NULL) THEN
        (EXTRACT(epoch FROM (now() - latest_scan.completed_at)) / (86400)::numeric)
    ELSE
        NULL::numeric
    END AS days_since_scan
FROM (public.view_systems_current_state sys
    LEFT JOIN ( SELECT DISTINCT ON (d.derivation_name)
            d.derivation_name AS hostname,
            cs.completed_at,
            cs.scanner_name,
            cs.total_packages,
            cs.total_vulnerabilities,
            cs.critical_count,
            cs.high_count,
            cs.medium_count,
            cs.low_count
        FROM (public.derivations d
            JOIN public.cve_scans cs ON (d.id = cs.derivation_id))
    WHERE ((d.derivation_type = 'nixos'::text)
        AND (d.status = ANY (ARRAY['build-complete'::text, 'complete'::text]))
        AND (cs.completed_at IS NOT NULL))
ORDER BY
    d.derivation_name,
    cs.completed_at DESC) latest_scan ON (sys.hostname = latest_scan.hostname))
ORDER BY
    sys.hostname;

-- Update view_systems_status_table
CREATE OR REPLACE VIEW public.view_systems_status_table AS
WITH current_state AS (
    SELECT DISTINCT ON (system_states.hostname)
        system_states.hostname,
        system_states.derivation_path,
        system_states."timestamp" AS current_deployed_at
    FROM
        public.system_states
    ORDER BY
        system_states.hostname,
        system_states."timestamp" DESC
),
current_eval AS (
    SELECT
        d.derivation_path,
        d.derivation_name,
        d.derivation_type,
        d.status,
        d.attempt_count,
        d.completed_at,
        d.commit_id
    FROM
        public.derivations d
    WHERE (d.derivation_type = 'nixos'::text)
),
current_commit AS (
    SELECT
        commits.id,
        commits.git_commit_hash,
        commits.commit_timestamp
    FROM
        public.commits
),
latest_commit AS (
    SELECT DISTINCT ON (s.hostname)
        s.hostname,
        c.git_commit_hash AS latest_commit_hash,
        c.commit_timestamp AS latest_commit_timestamp
    FROM ((public.systems s
            JOIN public.flakes f ON (s.flake_id = f.id))
        JOIN public.commits c ON (f.id = c.flake_id))
ORDER BY
    s.hostname,
    c.commit_timestamp DESC
),
joined AS (
    SELECT
        s.hostname,
        vcs.agent_version AS version,
        (vcs.uptime_days || 'd'::text) AS uptime,
    vcs.ip_address,
    vcs.last_seen,
    cs.current_deployed_at,
    cs.derivation_path AS current_derivation_path,
    ce.status AS current_eval_status,
    ce.attempt_count AS current_eval_attempts,
    ce.completed_at AS current_eval_completed,
    cc.git_commit_hash AS current_commit_hash,
    cc.commit_timestamp AS current_commit_timestamp,
    lc.latest_commit_hash,
    lc.latest_commit_timestamp
FROM (((((public.systems s
                LEFT JOIN current_state cs ON (s.hostname = cs.hostname))
            LEFT JOIN current_eval ce ON (((ce.derivation_path = cs.derivation_path)
                        AND (ce.derivation_name = s.hostname))))
        LEFT JOIN current_commit cc ON (ce.commit_id = cc.id))
        LEFT JOIN latest_commit lc ON (s.hostname = lc.hostname))
        LEFT JOIN public.view_systems_current_state vcs ON (s.hostname = vcs.hostname)))
SELECT
    hostname,
    CASE WHEN (current_derivation_path IS NULL) THEN
        '⚫'::text
    WHEN (current_commit_hash IS NULL) THEN
        '🟤'::text
    WHEN (current_commit_hash = latest_commit_hash) THEN
        '🟢'::text
    ELSE
        '🔴'::text
    END AS status,
    CASE WHEN (current_derivation_path IS NULL) THEN
        'Offline'::text
    WHEN (current_commit_hash IS NULL) THEN
        'Unknown State'::text
    WHEN (current_commit_hash = latest_commit_hash) THEN
        'Up to Date'::text
    ELSE
        'Outdated'::text
    END AS status_text,
    CASE WHEN (last_seen IS NULL) THEN
        'never'::text
    ELSE
        concat(((EXTRACT(epoch FROM (now() - last_seen)))::integer / 60), 'm ago')
    END AS last_seen,
    version,
    uptime,
    ip_address
FROM
    joined
ORDER BY
    CASE WHEN (current_commit_hash = latest_commit_hash) THEN
        1
    WHEN (current_commit_hash IS NULL) THEN
        4
    ELSE
        3
    END,
    CASE WHEN (last_seen IS NULL) THEN
        'never'::text
    ELSE
        concat(((EXTRACT(epoch FROM (now() - last_seen)))::integer / 60), 'm ago')
    END DESC;

-- Remove the compatibility views we created in migration 005 since we've updated everything
DROP VIEW IF EXISTS nix_packages;

DROP VIEW IF EXISTS evaluation_targets;


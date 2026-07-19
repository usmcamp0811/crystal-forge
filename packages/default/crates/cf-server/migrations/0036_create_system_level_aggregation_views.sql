-- View 1: System Build Progress Aggregation
-- Rolls up individual derivation build statuses to system-level progress
CREATE OR REPLACE VIEW public.view_system_build_progress AS
WITH nixos_systems AS (
    -- Get all NixOS system derivations (the main systems)
    SELECT
        d.id AS system_derivation_id,
        d.derivation_name AS system_name,
        d.commit_id,
        d.derivation_path AS system_derivation_path,
        ds.name AS system_status,
        ds.is_terminal,
        ds.is_success
    FROM
        public.derivations d
        JOIN public.derivation_statuses ds ON d.status_id = ds.id
    WHERE
        d.derivation_type = 'nixos'
),
component_stats AS (
    -- Aggregate component (package) derivations per system
    SELECT
        ns.system_derivation_id,
        ns.system_name,
        ns.commit_id,
        ns.system_status,
        ns.is_terminal AS system_is_terminal,
        ns.is_success AS system_is_success,
        -- Component counts
        COUNT(pkg.id) AS total_components,
        COUNT(pkg.id) FILTER (WHERE pkg_ds.name = 'dry-run-complete') AS components_evaluated,
        COUNT(pkg.id) FILTER (WHERE pkg_ds.name = 'build-complete') AS components_built,
        COUNT(pkg.id) FILTER (WHERE pkg_ds.name IN ('dry-run-failed', 'build-failed')) AS components_failed,
        COUNT(pkg.id) FILTER (WHERE pkg_ds.name IN ('build-pending', 'build-in-progress')) AS components_building,
        -- Build progress percentage
        CASE WHEN COUNT(pkg.id) = 0 THEN
            100.0 -- No components means system-only
        ELSE
            ROUND((COUNT(pkg.id) FILTER (WHERE pkg_ds.name = 'build-complete') * 100.0) / COUNT(pkg.id), 1)
        END AS build_progress_percent,
        -- Overall system readiness
        CASE WHEN ns.system_status = 'build-complete'
            AND COUNT(pkg.id) FILTER (WHERE pkg_ds.name != 'build-complete') = 0 THEN
            'ready'
        WHEN COUNT(pkg.id) FILTER (WHERE pkg_ds.name IN ('dry-run-failed', 'build-failed')) > 0 THEN
            'failed'
        WHEN COUNT(pkg.id) FILTER (WHERE pkg_ds.name IN ('build-pending', 'build-in-progress')) > 0 THEN
            'building'
        ELSE
            'evaluating'
        END AS overall_status
    FROM
        nixos_systems ns
        LEFT JOIN public.derivation_dependencies dd ON ns.system_derivation_id = dd.derivation_id
        LEFT JOIN public.derivations pkg ON dd.depends_on_id = pkg.id
            AND pkg.derivation_type = 'package'
        LEFT JOIN public.derivation_statuses pkg_ds ON pkg.status_id = pkg_ds.id
    GROUP BY
        ns.system_derivation_id,
        ns.system_name,
        ns.commit_id,
        ns.system_status,
        ns.is_terminal,
        ns.is_success
)
SELECT
    system_name,
    commit_id,
    system_status,
    overall_status,
    total_components,
    components_evaluated,
    components_built,
    components_failed,
    components_building,
    build_progress_percent,
    system_is_terminal,
    system_is_success
FROM
    component_stats
ORDER BY
    system_name;

COMMENT ON VIEW public.view_system_build_progress IS 'Aggregates individual derivation build progress to system-level summaries';

-- View 2: System Vulnerability Aggregation
-- Rolls up individual package CVE scans to system-level vulnerability counts
CREATE OR REPLACE VIEW public.view_system_vulnerability_summary AS
WITH nixos_systems AS (
    SELECT
        d.id AS system_derivation_id,
        d.derivation_name AS system_name,
        d.commit_id
    FROM
        public.derivations d
    WHERE
        d.derivation_type = 'nixos'
),
system_packages AS (
    -- Get all packages that belong to each system
    SELECT
        ns.system_derivation_id,
        ns.system_name,
        ns.commit_id,
        pkg.id AS package_derivation_id,
        pkg.derivation_name AS package_name,
        pkg.pname,
        pkg.version
    FROM
        nixos_systems ns
        LEFT JOIN public.derivation_dependencies dd ON ns.system_derivation_id = dd.derivation_id
        LEFT JOIN public.derivations pkg ON dd.depends_on_id = pkg.id
            AND pkg.derivation_type = 'package'
),
vulnerability_stats AS (
    -- Aggregate vulnerability counts per system
    SELECT
        sp.system_name,
        sp.commit_id,
        COUNT(DISTINCT sp.package_derivation_id) AS total_packages_scanned,
        COUNT(DISTINCT pv.cve_id) AS total_vulnerabilities,
        COUNT(DISTINCT pv.cve_id) FILTER (WHERE c.cvss_v3_score >= 9.0) AS critical_count,
        COUNT(DISTINCT pv.cve_id) FILTER (WHERE c.cvss_v3_score >= 7.0
            AND c.cvss_v3_score < 9.0) AS high_count,
        COUNT(DISTINCT pv.cve_id) FILTER (WHERE c.cvss_v3_score >= 4.0
            AND c.cvss_v3_score < 7.0) AS medium_count,
        COUNT(DISTINCT pv.cve_id) FILTER (WHERE c.cvss_v3_score < 4.0
            AND c.cvss_v3_score IS NOT NULL) AS low_count,
        COUNT(DISTINCT pv.cve_id) FILTER (WHERE c.cvss_v3_score IS NULL) AS unknown_count,
        COUNT(DISTINCT pv.cve_id) FILTER (WHERE pv.is_whitelisted = TRUE) AS whitelisted_count,
        -- Risk level based on highest severity
        CASE WHEN COUNT(DISTINCT pv.cve_id) FILTER (WHERE c.cvss_v3_score >= 9.0) > 0 THEN
            'CRITICAL'
        WHEN COUNT(DISTINCT pv.cve_id) FILTER (WHERE c.cvss_v3_score >= 7.0) > 0 THEN
            'HIGH'
        WHEN COUNT(DISTINCT pv.cve_id) FILTER (WHERE c.cvss_v3_score >= 4.0) > 0 THEN
            'MEDIUM'
        WHEN COUNT(DISTINCT pv.cve_id) FILTER (WHERE c.cvss_v3_score > 0) > 0 THEN
            'LOW'
        WHEN COUNT(DISTINCT pv.cve_id) = 0 THEN
            'CLEAN'
        ELSE
            'UNKNOWN'
        END AS risk_level,
        MAX(cs.completed_at) AS last_scan_completed
    FROM
        system_packages sp
    LEFT JOIN public.package_vulnerabilities pv ON sp.package_derivation_id = pv.derivation_id
        LEFT JOIN public.cves c ON pv.cve_id = c.id
        LEFT JOIN public.cve_scans cs ON sp.system_derivation_id = cs.derivation_id
    GROUP BY
        sp.system_name,
        sp.commit_id
)
SELECT
    system_name,
    commit_id,
    total_packages_scanned,
    total_vulnerabilities,
    critical_count,
    high_count,
    medium_count,
    low_count,
    unknown_count,
    whitelisted_count,
    risk_level,
    last_scan_completed
FROM
    vulnerability_stats
ORDER BY
    CASE risk_level
    WHEN 'CRITICAL' THEN
        1
    WHEN 'HIGH' THEN
        2
    WHEN 'MEDIUM' THEN
        3
    WHEN 'LOW' THEN
        4
    WHEN 'CLEAN' THEN
        5
    ELSE
        6
    END,
    total_vulnerabilities DESC;

COMMENT ON VIEW public.view_system_vulnerability_summary IS 'Aggregates individual package vulnerabilities to system-level security summaries';

-- View 3: System Deployment Readiness
-- Combines build progress and vulnerability data for deployment decisions
CREATE OR REPLACE VIEW public.view_system_deployment_readiness AS
WITH deployment_analysis AS (
    SELECT
        bp.system_name,
        bp.commit_id,
        c.git_commit_hash,
        c.commit_timestamp,
        f.name AS flake_name,
        -- Build readiness
        bp.overall_status AS build_status,
        bp.build_progress_percent,
        bp.total_components,
        bp.components_failed,
        -- Security readiness
        COALESCE(vs.risk_level, 'NOT_SCANNED') AS security_risk_level,
        COALESCE(vs.total_vulnerabilities, 0) AS total_vulnerabilities,
        COALESCE(vs.critical_count, 0) AS critical_vulnerabilities,
        COALESCE(vs.high_count, 0) AS high_vulnerabilities,
        vs.last_scan_completed,
        -- Overall deployment readiness
        CASE WHEN bp.overall_status != 'ready' THEN
            'BUILD_NOT_READY'
        WHEN COALESCE(vs.risk_level, 'NOT_SCANNED') = 'NOT_SCANNED' THEN
            'SCAN_REQUIRED'
        WHEN COALESCE(vs.critical_count, 0) > 0 THEN
            'SECURITY_RISK'
        WHEN COALESCE(vs.high_count, 0) > 10 THEN
            'HIGH_VULN_COUNT' -- Configurable threshold
        ELSE
            'READY'
        END AS deployment_readiness,
        -- Current deployment status
        CASE WHEN current_deploy.hostname IS NOT NULL THEN
            'DEPLOYED'
        ELSE
            'NOT_DEPLOYED'
        END AS deployment_status,
        current_deploy.hostname AS deployed_to_system,
        current_deploy.last_deployed
    FROM
        public.view_system_build_progress bp
        LEFT JOIN public.commits c ON bp.commit_id = c.id
        LEFT JOIN public.flakes f ON c.flake_id = f.id
        LEFT JOIN public.view_system_vulnerability_summary vs ON bp.system_name = vs.system_name
            AND bp.commit_id = vs.commit_id
        LEFT JOIN (
            -- Check if this system+commit is currently deployed
            SELECT DISTINCT ON (s.hostname)
                s.hostname,
                d.derivation_name AS system_name,
                d.commit_id,
                ss.timestamp AS last_deployed
            FROM
                public.systems s
                JOIN public.derivations d ON s.hostname = d.derivation_name
                    AND d.derivation_type = 'nixos'
                JOIN public.system_states ss ON d.derivation_path = ss.derivation_path
                    AND s.hostname = ss.hostname
                ORDER BY
                    s.hostname,
                    ss.timestamp DESC) current_deploy ON bp.system_name = current_deploy.system_name
                AND bp.commit_id = current_deploy.commit_id
)
        SELECT
            *
        FROM
            deployment_analysis
        ORDER BY
            CASE deployment_readiness
            WHEN 'READY' THEN
                1
            WHEN 'HIGH_VULN_COUNT' THEN
                2
            WHEN 'SECURITY_RISK' THEN
                3
            WHEN 'SCAN_REQUIRED' THEN
                4
            WHEN 'BUILD_NOT_READY' THEN
                5
            END,
            commit_timestamp DESC;

COMMENT ON VIEW public.view_system_deployment_readiness IS 'Combines build and security data to determine system deployment readiness';


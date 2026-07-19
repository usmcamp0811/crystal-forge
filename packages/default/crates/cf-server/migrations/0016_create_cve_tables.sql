-- CVE integration schema for existing NixOS management database
-- ============================================================================
-- CVE CORE TABLES
-- ============================================================================
-- CVE definitions from vulnerability databases
CREATE TABLE cves (
    id varchar(20) PRIMARY KEY, -- CVE-YYYY-NNNNN
    cvss_v3_score DECIMAL(3, 1),
    cvss_v2_score DECIMAL(3, 1),
    description text,
    published_date date,
    modified_date date,
    -- severity computed from cvss_v3_score, no redundant storage
    vector varchar(100), -- CVSS vector string
    cwe_id varchar(20), -- Common Weakness Enumeration
    metadata jsonb, -- Additional CVE details (references, etc.)
    created_at timestamptz DEFAULT NOW(),
    updated_at timestamptz DEFAULT NOW()
);

-- Nix packages with version information
CREATE TABLE nix_packages (
    derivation_path text PRIMARY KEY, -- Natural primary key, already unique
    name varchar(255) NOT NULL, -- Full package name for display
    pname varchar(255) NOT NULL, -- Package name for CVE matching
    version varchar(100) NOT NULL, -- Version for vulnerability assessment
    created_at timestamptz DEFAULT NOW()
);

-- Link packages to CVEs with vulnerability details
CREATE TABLE package_vulnerabilities (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid (),
    derivation_path text REFERENCES nix_packages (derivation_path) ON DELETE CASCADE,
    cve_id varchar(20) REFERENCES cves (id) ON DELETE CASCADE,
    is_whitelisted boolean DEFAULT FALSE,
    whitelist_reason text,
    whitelist_expires_at timestamptz,
    fixed_version varchar(100), -- Version where CVE is fixed
    detection_method varchar(50) DEFAULT 'vulnix', -- vulnix, trivy, osv, etc.
    created_at timestamptz DEFAULT NOW(),
    updated_at timestamptz DEFAULT NOW(),
    UNIQUE (derivation_path, cve_id)
);

-- Enhanced CVE scans table with simple attempt tracking
CREATE TABLE cve_scans (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid (),
    evaluation_target_id integer REFERENCES evaluation_targets (id) ON DELETE CASCADE NOT NULL,
    scheduled_at timestamptz DEFAULT NOW(),
    completed_at timestamptz,
    status varchar(20), -- 'pending', 'in_progress', 'completed', 'failed', etc.
    attempts integer DEFAULT 0, -- increment each time we try to scan
    scanner_name varchar(50) NOT NULL, -- vulnix, trivy, grype, etc.
    scanner_version varchar(50),
    total_packages integer NOT NULL DEFAULT 0,
    total_vulnerabilities integer NOT NULL DEFAULT 0,
    critical_count integer NOT NULL DEFAULT 0,
    high_count integer NOT NULL DEFAULT 0,
    medium_count integer NOT NULL DEFAULT 0,
    low_count integer NOT NULL DEFAULT 0,
    scan_duration_ms integer,
    scan_metadata jsonb, -- scanner-specific data, error details if needed
    created_at timestamptz DEFAULT NOW()
);

-- Link CVE scans to discovered packages (many-to-many)
CREATE TABLE scan_packages (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid (),
    scan_id uuid REFERENCES cve_scans (id) ON DELETE CASCADE,
    derivation_path text REFERENCES nix_packages (derivation_path) ON DELETE CASCADE,
    is_runtime_dependency boolean DEFAULT TRUE,
    dependency_depth integer DEFAULT 0, -- 0 = direct, 1+ = transitive
    created_at timestamptz DEFAULT NOW(),
    CONSTRAINT uq_scan_package UNIQUE (scan_id, derivation_path)
);

-- migrate update status from 'complete' to 'dry-run-complete'
BEGIN;
-- Add error_message column (referenced in your Rust code but missing from schema)
ALTER TABLE evaluation_targets
    ADD COLUMN IF NOT EXISTS error_message text;
-- Drop the old check constraint
ALTER TABLE evaluation_targets
    DROP CONSTRAINT IF EXISTS valid_status;
-- Add new check constraint with all the status values from EvaluationStatus enum
ALTER TABLE evaluation_targets
    ADD CONSTRAINT valid_status CHECK (status = ANY (ARRAY['dry-run-pending'::text, 'dry-run-inprogress'::text, 'dry-run-complete'::text, 'dry-run-failed'::text, 'build-pending'::text, 'build-inprogress'::text, 'build-complete'::text, 'build-failed'::text,
    -- Keep old values for backward compatibility during transition
    'pending'::text, 'queued'::text, 'in-progress'::text, 'complete'::text, 'failed'::text]));
-- Update existing records to use new status values
-- Map old statuses to new ones based on whether they have derivation_path
UPDATE
    evaluation_targets
SET
    status = CASE WHEN status = 'pending'
        AND derivation_path IS NULL THEN
        'dry-run-pending'
    WHEN status = 'pending'
        AND derivation_path IS NOT NULL THEN
        'build-pending'
    WHEN status = 'queued'
        AND derivation_path IS NULL THEN
        'dry-run-pending'
    WHEN status = 'queued'
        AND derivation_path IS NOT NULL THEN
        'build-pending'
    WHEN status = 'in-progress'
        AND derivation_path IS NULL THEN
        'dry-run-inprogress'
    WHEN status = 'in-progress'
        AND derivation_path IS NOT NULL THEN
        'build-inprogress'
    WHEN status = 'complete'
        AND derivation_path IS NULL THEN
        'dry-run-complete'
    WHEN status = 'complete'
        AND derivation_path IS NOT NULL THEN
        'build-complete'
    WHEN status = 'failed'
        AND derivation_path IS NULL THEN
        'dry-run-failed'
    WHEN status = 'failed'
        AND derivation_path IS NOT NULL THEN
        'build-failed'
    ELSE
        status -- Keep new statuses as-is
    END
WHERE
    status IN ('pending', 'queued', 'in-progress', 'complete', 'failed');
-- Update the default value for new records
ALTER TABLE evaluation_targets
    ALTER COLUMN status SET DEFAULT 'dry-run-pending';
COMMIT;

-- ============================================================================
-- INDEXES FOR PERFORMANCE
-- ============================================================================
-- CVE indexes
CREATE INDEX idx_cves_cvss_v3_score ON cves (cvss_v3_score);

CREATE INDEX idx_cves_published ON cves (published_date);

-- Nix package indexes
CREATE INDEX idx_nix_packages_pname_version ON nix_packages (pname, version);

CREATE INDEX idx_nix_packages_name ON nix_packages (name);

-- Vulnerability relationship indexes
CREATE INDEX idx_package_vulnerabilities_derivation ON package_vulnerabilities (derivation_path);

CREATE INDEX idx_package_vulnerabilities_cve ON package_vulnerabilities (cve_id);

CREATE INDEX idx_package_vulnerabilities_whitelist ON package_vulnerabilities (is_whitelisted);

-- Scan indexes
CREATE INDEX idx_cve_scans_evaluation_target ON cve_scans (evaluation_target_id);

CREATE INDEX idx_cve_scans_scanner ON cve_scans (scanner_name);

CREATE INDEX idx_cve_scans_completed_at ON cve_scans (completed_at);

-- Scan package relationship indexes
CREATE INDEX idx_scan_packages_scan ON scan_packages (scan_id);

CREATE INDEX idx_scan_packages_derivation ON scan_packages (derivation_path);

-- ============================================================================
-- FUNCTIONS
-- ============================================================================
CREATE OR REPLACE FUNCTION severity_from_cvss (score DECIMAL)
    RETURNS varchar (
        20
)
    AS $$
BEGIN
    IF score IS NULL THEN
        RETURN 'UNKNOWN';
    ELSIF score >= 9.0 THEN
        RETURN 'CRITICAL';
    ELSIF score >= 7.0 THEN
        RETURN 'HIGH';
    ELSIF score >= 4.0 THEN
        RETURN 'MEDIUM';
    ELSE
        RETURN 'LOW';
    END IF;
END;
$$
LANGUAGE plpgsql
IMMUTABLE;

-- Get latest CVE scan for a target
CREATE OR REPLACE FUNCTION get_latest_cve_scan (target_name text)
    RETURNS TABLE (
        scan_id uuid,
        completed_at timestamptz,
        total_vulnerabilities integer,
        critical_count integer,
        high_count integer
    )
    AS $$
BEGIN
    RETURN QUERY
    SELECT
        cs.id,
        cs.completed_at,
        cs.total_vulnerabilities,
        cs.critical_count,
        cs.high_count
    FROM
        evaluation_targets et
        JOIN cve_scans cs ON et.id = cs.evaluation_target_id
    WHERE
        et.target_name = target_name
        AND et.target_type = 'nixos'
        AND et.status = 'complete'
        AND cs.completed_at IS NOT NULL
    ORDER BY
        cs.completed_at DESC
    LIMIT 1;
END;
$$
LANGUAGE plpgsql;

-- ============================================================================
-- VIEWS
-- ============================================================================
-- Systems CVE summary view
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
            AND et.status = 'complete'
            AND cs.completed_at IS NOT NULL
        ORDER BY
            et.target_name,
            cs.completed_at DESC) latest_scan ON sys.hostname = latest_scan.hostname
ORDER BY
    sys.hostname;

-- System vulnerabilities view
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
    AND et.status = 'complete'
    AND scan.completed_at IS NOT NULL
    AND NOT pv.is_whitelisted
ORDER BY
    et.target_name,
    c.cvss_v3_score DESC NULLS LAST;

-- Environment security posture view
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
        AND et.status = 'complete'
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
            AND et.status = 'complete'
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

-- CVE trends over 7 days
CREATE OR REPLACE VIEW view_cve_trends_7d AS
SELECT
    DATE(completed_at) AS scan_date,
    COUNT(DISTINCT cs.evaluation_target_id) AS targets_scanned,
    AVG(cs.total_vulnerabilities) AS avg_vulnerabilities,
    AVG(cs.critical_count) AS avg_critical,
    AVG(cs.high_count) AS avg_high,
    SUM(cs.total_vulnerabilities) AS total_vulnerabilities,
    SUM(cs.critical_count) AS total_critical,
    SUM(cs.high_count) AS total_high
FROM
    cve_scans cs
WHERE
    completed_at >= CURRENT_DATE - INTERVAL '7 days'
    AND completed_at IS NOT NULL
GROUP BY
    DATE(completed_at)
ORDER BY
    scan_date;

-- Critical vulnerabilities alert view
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
    AND et.status = 'complete'
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


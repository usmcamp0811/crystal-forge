-- CVE views and indexes for advanced CVE dashboard
-- ============================================================================
-- SCHEMA EXTENSIONS
-- ============================================================================

-- Add exploited flag to cves table
ALTER TABLE cves ADD COLUMN IF NOT EXISTS exploited BOOLEAN DEFAULT FALSE;

-- Add index for exploited flag
CREATE INDEX IF NOT EXISTS idx_cves_exploited ON cves (exploited) WHERE exploited = TRUE;

-- ============================================================================
-- PERFORMANCE INDEXES
-- ============================================================================

-- Index for CVE status filtering
CREATE INDEX IF NOT EXISTS idx_package_vulnerabilities_fixed_version 
  ON package_vulnerabilities (fixed_version) WHERE fixed_version IS NOT NULL;

-- Index for triage status lookups
CREATE INDEX IF NOT EXISTS idx_system_cve_justifications_category 
  ON system_cve_justifications (category);

CREATE INDEX IF NOT EXISTS idx_system_cve_justifications_cve_id 
  ON system_cve_justifications (cve_id);

-- Composite index for common CVE queries
CREATE INDEX IF NOT EXISTS idx_cves_severity_score 
  ON cves (cvss_v3_score DESC) WHERE cvss_v3_score IS NOT NULL;

-- ============================================================================
-- CVE LIST VIEW WITH METADATA
-- ============================================================================

CREATE OR REPLACE VIEW view_cve_list_with_metadata AS
WITH latest_scans AS (
  -- Get the most recent completed scan per system
  SELECT DISTINCT ON (s.id, c.id)
    s.id as system_id,
    c.id as cve_id,
    cs.completed_at,
    e.name as environment_name
  FROM systems s
  JOIN evaluation_targets et ON s.hostname = et.target_name AND et.target_type = 'nixos'
  JOIN cve_scans cs ON et.id = cs.evaluation_target_id AND cs.completed_at IS NOT NULL
  JOIN scan_packages sp ON cs.id = sp.scan_id
  JOIN nix_packages np ON sp.derivation_path = np.derivation_path
  JOIN package_vulnerabilities pv ON np.derivation_path = pv.derivation_path AND NOT pv.is_whitelisted
  JOIN cves c ON pv.cve_id = c.id
  LEFT JOIN environments e ON s.environment_id = e.id
  WHERE s.is_active = TRUE
  ORDER BY s.id, c.id, cs.completed_at DESC
),
cve_triage_status AS (
  -- Determine fleet-wide triage status for each CVE
  -- If ANY system has accepted_risk, CVE is 'accepted'
  -- If ANY system has patch_scheduled, CVE is 'scheduled'
  -- Otherwise 'outstanding'
  SELECT 
    cve_id,
    CASE
      WHEN bool_or(category = 'accepted_risk') THEN 'accepted'
      WHEN bool_or(category = 'patch_scheduled') THEN 'scheduled'
      ELSE 'outstanding'
    END as triage_status
  FROM system_cve_justifications
  GROUP BY cve_id
)
SELECT DISTINCT
  c.id as cve_id,
  c.cvss_v3_score,
  severity_from_cvss(c.cvss_v3_score) as severity,
  COALESCE(NULLIF(TRIM(c.description), ''), c.id) as title,
  c.vector as cvss_vector,
  c.published_date,
  c.exploited,
  np.pname as package_name,
  np.version as installed_version,
  pv.fixed_version,
  CASE WHEN pv.fixed_version IS NOT NULL THEN 'fix_available' ELSE 'open' END as fix_status,
  COUNT(DISTINCT ls.system_id) as affected_count,
  ARRAY_AGG(DISTINCT ls.environment_name ORDER BY ls.environment_name) FILTER (WHERE ls.environment_name IS NOT NULL) as affected_environments,
  MIN(ls.completed_at) as first_seen,
  MAX(ls.completed_at) as last_seen,
  COALESCE(EXTRACT(EPOCH FROM (NOW() - c.published_date))/86400, 0)::INTEGER as age_days,
  COALESCE(cts.triage_status, 'outstanding') as triage_status
FROM cves c
LEFT JOIN package_vulnerabilities pv ON c.id = pv.cve_id AND NOT pv.is_whitelisted
LEFT JOIN nix_packages np ON pv.derivation_path = np.derivation_path
LEFT JOIN latest_scans ls ON c.id = ls.cve_id
LEFT JOIN cve_triage_status cts ON c.id = cts.cve_id
WHERE c.id IS NOT NULL
GROUP BY c.id, c.cvss_v3_score, c.description, c.vector, c.published_date, c.exploited,
         np.pname, np.version, pv.fixed_version, cts.triage_status;

-- ============================================================================
-- CVEs GROUPED BY PACKAGE VIEW
-- ============================================================================

CREATE OR REPLACE VIEW view_cves_grouped_by_package AS
WITH package_cve_stats AS (
  SELECT
    package_name,
    COUNT(*) as cve_count,
    COUNT(*) FILTER (WHERE severity = 'CRITICAL') as critical_count,
    COUNT(*) FILTER (WHERE severity = 'HIGH') as high_count,
    COUNT(*) FILTER (WHERE severity = 'MEDIUM') as medium_count,
    COUNT(*) FILTER (WHERE severity = 'LOW') as low_count,
    COUNT(DISTINCT UNNEST(affected_environments)) as environments_count,
    SUM(affected_count) as total_affected_systems,
    COUNT(*) FILTER (WHERE fix_status = 'fix_available') as fixable_count,
    COUNT(*) FILTER (WHERE triage_status = 'outstanding') as outstanding_count,
    COUNT(*) FILTER (WHERE exploited = TRUE) as exploited_count,
    MAX(cvss_v3_score) as max_cvss,
    -- Severity score for sorting (critical = 1000, high = 100, medium = 10, low = 1)
    SUM(CASE severity
      WHEN 'CRITICAL' THEN 1000
      WHEN 'HIGH' THEN 100
      WHEN 'MEDIUM' THEN 10
      WHEN 'LOW' THEN 1
      ELSE 0
    END) as severity_score
  FROM view_cve_list_with_metadata
  WHERE package_name IS NOT NULL
  GROUP BY package_name
)
SELECT * FROM package_cve_stats
ORDER BY severity_score DESC, max_cvss DESC NULLS LAST;

-- ============================================================================
-- FLEET-WIDE CVE STATISTICS VIEW
-- ============================================================================

CREATE OR REPLACE VIEW view_cve_fleet_stats AS
SELECT
  COUNT(*) as total_cves,
  COUNT(*) FILTER (WHERE severity = 'CRITICAL') as critical,
  COUNT(*) FILTER (WHERE severity = 'HIGH') as high,
  COUNT(*) FILTER (WHERE severity = 'MEDIUM') as medium,
  COUNT(*) FILTER (WHERE severity = 'LOW') as low,
  COUNT(*) FILTER (WHERE exploited = TRUE) as exploited,
  COUNT(*) FILTER (WHERE fix_status = 'fix_available') as fixable,
  COUNT(DISTINCT UNNEST(affected_environments)) as environments_affected,
  SUM(affected_count) as total_system_cve_instances,
  COUNT(*) FILTER (WHERE triage_status = 'outstanding') as outstanding,
  COUNT(*) FILTER (WHERE triage_status = 'accepted') as accepted,
  COUNT(*) FILTER (WHERE triage_status = 'scheduled') as scheduled
FROM view_cve_list_with_metadata;

-- ============================================================================
-- COMMENTS
-- ============================================================================

COMMENT ON VIEW view_cve_list_with_metadata IS 
  'CVE list with aggregated metadata including affected systems, triage status, and fix availability';

COMMENT ON VIEW view_cves_grouped_by_package IS 
  'CVEs grouped by package with severity counts and scoring for dashboard display';

COMMENT ON VIEW view_cve_fleet_stats IS 
  'Fleet-wide CVE statistics for dashboard summary cards';

COMMENT ON COLUMN cves.exploited IS 
  'Indicates if CVE is actively exploited in the wild (requires external CISA KEV integration)';

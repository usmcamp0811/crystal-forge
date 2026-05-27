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
-- Uses the same join chain as view_system_vulnerabilities:
--   derivations (nixos) → cve_scans → scan_packages
--     → derivations (package) → package_vulnerabilities → cves
--   systems.hostname = derivations.derivation_name (nixos type)
-- ============================================================================

CREATE OR REPLACE VIEW view_cve_list_with_metadata AS
WITH latest_scans AS (
  -- One row per (system, cve): most recent completed scan
  SELECT DISTINCT ON (s.id, c.id)
    s.id          AS system_id,
    s.hostname,
    c.id          AS cve_id,
    scan.completed_at,
    e.name        AS environment_name,
    pkg_d.pname   AS package_name,
    pkg_d.version AS installed_version,
    pv.fixed_version,
    CASE WHEN pv.fixed_version IS NOT NULL THEN 'fix_available' ELSE 'open' END AS fix_status
  FROM systems s
  JOIN derivations d
    ON s.hostname = d.derivation_name
   AND d.derivation_type = 'nixos'
  JOIN derivation_statuses ds
    ON d.status_id = ds.id
   AND ds.name IN ('build-complete','complete')
  JOIN cve_scans scan
    ON d.id = scan.derivation_id
   AND scan.completed_at IS NOT NULL
  JOIN scan_packages sp
    ON scan.id = sp.scan_id
  JOIN derivations pkg_d
    ON sp.derivation_id = pkg_d.id
   AND pkg_d.derivation_type = 'package'
  JOIN package_vulnerabilities pv
    ON pkg_d.id = pv.derivation_id
   AND NOT pv.is_whitelisted
  JOIN cves c
    ON pv.cve_id::text = c.id::text
  LEFT JOIN environments e
    ON s.environment_id = e.id
  WHERE s.is_active = TRUE
  ORDER BY s.id, c.id, scan.completed_at DESC
),
cve_triage_status AS (
  -- Fleet-wide triage: accepted_risk wins > patch_scheduled > outstanding
  SELECT
    cve_id,
    CASE
      WHEN bool_or(category = 'accepted_risk')   THEN 'accepted'
      WHEN bool_or(category = 'patch_scheduled') THEN 'scheduled'
      ELSE 'outstanding'
    END AS triage_status
  FROM system_cve_justifications
  GROUP BY cve_id
)
SELECT
  c.id                                                                    AS cve_id,
  c.cvss_v3_score,
  severity_from_cvss(c.cvss_v3_score)                                    AS severity,
  COALESCE(NULLIF(TRIM(c.description), ''), c.id)                        AS title,
  c.vector                                                                AS cvss_vector,
  c.published_date,
  c.exploited,
  ls.package_name,
  ls.installed_version,
  ls.fixed_version,
  ls.fix_status,
  COUNT(DISTINCT ls.system_id)                                           AS affected_count,
  ARRAY_AGG(DISTINCT ls.environment_name ORDER BY ls.environment_name)
    FILTER (WHERE ls.environment_name IS NOT NULL)                       AS affected_environments,
  MIN(ls.completed_at)                                                   AS first_seen,
  MAX(ls.completed_at)                                                   AS last_seen,
  COALESCE(EXTRACT(EPOCH FROM (NOW() - c.published_date))/86400, 0)::INTEGER AS age_days,
  COALESCE(cts.triage_status, 'outstanding')                             AS triage_status
FROM cves c
LEFT JOIN latest_scans ls
  ON c.id = ls.cve_id
LEFT JOIN cve_triage_status cts
  ON c.id = cts.cve_id
WHERE c.id IS NOT NULL
GROUP BY
  c.id, c.cvss_v3_score, c.description, c.vector, c.published_date, c.exploited,
  ls.package_name, ls.installed_version, ls.fixed_version, ls.fix_status,
  cts.triage_status;

-- ============================================================================
-- CVEs GROUPED BY PACKAGE VIEW
-- ============================================================================

CREATE OR REPLACE VIEW view_cves_grouped_by_package AS
SELECT
  package_name,
  COUNT(*)                                                        AS cve_count,
  COUNT(*) FILTER (WHERE severity = 'CRITICAL')                  AS critical_count,
  COUNT(*) FILTER (WHERE severity = 'HIGH')                      AS high_count,
  COUNT(*) FILTER (WHERE severity = 'MEDIUM')                    AS medium_count,
  COUNT(*) FILTER (WHERE severity = 'LOW')                       AS low_count,
  COUNT(DISTINCT ae) AS environments_count,
  SUM(affected_count)                                            AS total_affected_systems,
  COUNT(*) FILTER (WHERE fix_status = 'fix_available')           AS fixable_count,
  COUNT(*) FILTER (WHERE triage_status = 'outstanding')          AS outstanding_count,
  COUNT(*) FILTER (WHERE exploited = TRUE)                       AS exploited_count,
  MAX(cvss_v3_score)                                             AS max_cvss,
  SUM(CASE severity
        WHEN 'CRITICAL' THEN 1000
        WHEN 'HIGH'     THEN 100
        WHEN 'MEDIUM'   THEN 10
        WHEN 'LOW'      THEN 1
        ELSE 0
      END)                                                       AS severity_score
FROM view_cve_list_with_metadata
LEFT JOIN LATERAL UNNEST(affected_environments) AS ae ON TRUE
WHERE package_name IS NOT NULL
GROUP BY package_name
ORDER BY severity_score DESC, max_cvss DESC NULLS LAST;

-- ============================================================================
-- FLEET-WIDE CVE STATISTICS VIEW
-- ============================================================================

CREATE OR REPLACE VIEW view_cve_fleet_stats AS
SELECT
  COUNT(*)                                                               AS total_cves,
  COUNT(*) FILTER (WHERE severity = 'CRITICAL')                         AS critical,
  COUNT(*) FILTER (WHERE severity = 'HIGH')                             AS high,
  COUNT(*) FILTER (WHERE severity = 'MEDIUM')                           AS medium,
  COUNT(*) FILTER (WHERE severity = 'LOW')                              AS low,
  COUNT(*) FILTER (WHERE exploited = TRUE)                              AS exploited,
  COUNT(*) FILTER (WHERE fix_status = 'fix_available')                  AS fixable,
  (SELECT COUNT(DISTINCT e) FROM view_cve_list_with_metadata v2, UNNEST(v2.affected_environments) AS e) AS environments_affected,
  COALESCE(SUM(affected_count), 0)                                      AS systems_affected,
  COUNT(*) FILTER (WHERE triage_status = 'outstanding')                 AS outstanding,
  COUNT(*) FILTER (WHERE triage_status = 'accepted')                    AS accepted,
  COUNT(*) FILTER (WHERE triage_status = 'scheduled')                   AS scheduled
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

-- Scope fleet CVE triage status to fleet-wide justifications only.
--
-- system_cve_justifications supports both:
--   * fleet-wide rows (system_id IS NULL)
--   * per-system rows (system_id IS NOT NULL)
--
-- Fleet CVE list/grouped/stats views must derive top-level triage status from
-- fleet-wide rows only so a single system-scoped justification does not
-- incorrectly mark the whole CVE as accepted/scheduled.

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
  -- Fleet-wide triage only: accepted_risk wins > patch_scheduled > outstanding
  SELECT
    cve_id,
    CASE
      WHEN bool_or(category = 'accepted_risk')   THEN 'accepted'
      WHEN bool_or(category = 'patch_scheduled') THEN 'scheduled'
      ELSE 'outstanding'
    END AS triage_status
  FROM system_cve_justifications
  WHERE system_id IS NULL
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

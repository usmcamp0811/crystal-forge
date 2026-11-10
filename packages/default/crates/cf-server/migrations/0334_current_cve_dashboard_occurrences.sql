-- Admin CVE dashboard rows describe runtime exposure, not the newest artifact
-- with the same hostname. In particular, conversion back to managed must not
-- promote a retained configuration scan before fresh enrollment reports the
-- exact running output. Reuse the same Current authority as the main dashboard
-- and POA&M APIs rather than inventing another enrollment or scan selector.
CREATE VIEW view_current_cve_dashboard_occurrences AS
SELECT occurrence.system_id,
       system.hostname,
       occurrence.cve_id,
       public.severity_from_cvss(cve.cvss_v3_score) AS severity,
       cve.cvss_v3_score,
       occurrence.package_name,
       occurrence.package_name AS package_pname,
       occurrence.installed_version AS package_version,
       occurrence.completed_at,
       -- A fix hint is metadata only. It cannot add a finding or change its
       -- installed version, which come from immutable Current observations.
       (SELECT vulnerability.fixed_version
        FROM package_vulnerabilities vulnerability
        JOIN derivations package ON package.id = vulnerability.derivation_id
        WHERE package.derivation_path = occurrence.observed_derivation_path
          AND vulnerability.cve_id = occurrence.cve_id
          AND vulnerability.fixed_version IS NOT NULL
        ORDER BY vulnerability.fixed_version
        LIMIT 1) AS fixed_version
FROM view_current_exact_cve_occurrences occurrence
JOIN systems system ON system.id = occurrence.system_id
JOIN cves cve ON cve.id = occurrence.cve_id;

COMMENT ON VIEW view_current_cve_dashboard_occurrences IS
    'Admin CVE dashboard projection of Current-authorized immutable findings. Artifact scans alone never establish runtime exposure, including after re-enrollment.';

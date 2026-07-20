-- Bound CVE read amplification to the latest completed scan per hostname.
--
-- The previous view joined every completed historical scan. As scan history
-- accumulated, Systems, System Detail, Dashboard, and Environment queries all
-- repeatedly aggregated the full history through this shared view.

CREATE INDEX IF NOT EXISTS idx_cve_scans_latest_completed
    ON public.cve_scans (derivation_id, completed_at DESC, id DESC)
    WHERE status = 'completed' AND completed_at IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_derivations_nixos_hostname
    ON public.derivations (derivation_name, id, commit_id)
    WHERE derivation_type = 'nixos';

CREATE OR REPLACE VIEW public.view_system_vulnerabilities AS
WITH latest_scans AS (
    SELECT DISTINCT ON (d.derivation_name)
        d.derivation_name AS hostname,
        d.derivation_path AS evaluation_derivation_path,
        scan.id AS scan_id,
        scan.completed_at,
        scan.scanner_name,
        commits.git_commit_hash,
        flakes.name AS flake_name
    FROM public.derivations d
    JOIN public.derivation_statuses ds ON d.status_id = ds.id
    JOIN public.commits ON d.commit_id = commits.id
    JOIN public.flakes ON commits.flake_id = flakes.id
    JOIN public.cve_scans scan ON d.id = scan.derivation_id
    WHERE d.derivation_type = 'nixos'
      AND ds.name = ANY (ARRAY['build-complete'::text, 'complete'::text])
      AND scan.status = 'completed'
      AND scan.completed_at IS NOT NULL
    ORDER BY d.derivation_name, scan.completed_at DESC, scan.id DESC
)
SELECT
    ls.hostname,
    pkg_d.derivation_name AS package_name,
    pkg_d.pname AS package_pname,
    pkg_d.version AS package_version,
    pkg_d.derivation_path,
    c.id AS cve_id,
    c.cvss_v3_score,
    public.severity_from_cvss(c.cvss_v3_score) AS severity,
    c.description,
    pv.is_whitelisted,
    pv.whitelist_reason,
    pv.fixed_version,
    pv.detection_method,
    ls.completed_at,
    ls.scanner_name,
    ls.evaluation_derivation_path,
    ls.git_commit_hash,
    ls.flake_name
FROM latest_scans ls
JOIN public.scan_packages sp ON sp.scan_id = ls.scan_id
JOIN public.derivations pkg_d
  ON pkg_d.id = sp.derivation_id
 AND pkg_d.derivation_type = 'package'
JOIN public.package_vulnerabilities pv ON pv.derivation_id = pkg_d.id
JOIN public.cves c ON pv.cve_id::text = c.id::text
WHERE NOT pv.is_whitelisted
ORDER BY ls.hostname, c.cvss_v3_score DESC NULLS LAST;

-- These tables receive bursty inserts and conflict updates from CVE scans.
-- Lower per-table thresholds keep planner statistics current and prevent dead
-- tuples from accumulating between default percentage-based autovacuum runs.
ALTER TABLE public.derivations SET (
    autovacuum_vacuum_scale_factor = 0.01,
    autovacuum_vacuum_threshold = 500,
    autovacuum_analyze_scale_factor = 0.005,
    autovacuum_analyze_threshold = 500
);

ALTER TABLE public.package_vulnerabilities SET (
    autovacuum_vacuum_scale_factor = 0.02,
    autovacuum_vacuum_threshold = 250,
    autovacuum_analyze_scale_factor = 0.01,
    autovacuum_analyze_threshold = 250
);

ALTER TABLE public.scan_packages SET (
    autovacuum_vacuum_insert_scale_factor = 0.02,
    autovacuum_vacuum_insert_threshold = 1000,
    autovacuum_analyze_scale_factor = 0.01,
    autovacuum_analyze_threshold = 1000
);

ALTER TABLE public.cve_scans SET (
    autovacuum_vacuum_scale_factor = 0.02,
    autovacuum_vacuum_threshold = 100,
    autovacuum_analyze_scale_factor = 0.01,
    autovacuum_analyze_threshold = 100
);

-- Hotfix: ensure `public.view_system_vulnerabilities` exists in production.
--
-- Some environments have drift where this view is missing, causing
-- `/api/v1/cves/summary` and `/api/v1/cves/vulnerabilities` to fail with
-- `relation "view_system_vulnerabilities" does not exist`.
--
-- This migration is forward-safe and idempotent via CREATE OR REPLACE VIEW.

CREATE OR REPLACE VIEW public.view_system_vulnerabilities AS
SELECT
    d.derivation_name AS hostname,
    pkg_d.package_name,
    pkg_d.package_pname AS package_pname,
    pkg_d.package_version AS package_version,
    pkg_d.derivation_path,
    c.id AS cve_id,
    c.cvss_v3_score,
    public.severity_from_cvss(c.cvss_v3_score) AS severity,
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
FROM public.derivations d
JOIN public.commits ON d.commit_id = commits.id
JOIN public.flakes ON commits.flake_id = flakes.id
JOIN public.cve_scans scan ON d.id = scan.derivation_id
JOIN public.scan_packages sp ON scan.id = sp.scan_id
JOIN public.derivations pkg_d ON sp.derivation_id = pkg_d.id
JOIN public.package_vulnerabilities pv ON pkg_d.id = pv.derivation_id
JOIN public.cves c ON (pv.cve_id)::text = (c.id)::text
WHERE d.derivation_type = 'nixos'
  AND d.status = ANY (ARRAY['build-complete'::text, 'complete'::text])
  AND pkg_d.derivation_type = 'package'
  AND scan.completed_at IS NOT NULL
  AND NOT pv.is_whitelisted
ORDER BY d.derivation_name, c.cvss_v3_score DESC NULLS LAST;

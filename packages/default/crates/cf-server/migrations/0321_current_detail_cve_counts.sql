-- Detail runtime counts use the same exact Current authority as the list.
-- Replace only the CVE CTE. Inactive health/telemetry from 0320 stays unchanged.
DO $migration$
DECLARE
    definition text;
    legacy_counts text;
BEGIN
    definition := pg_get_viewdef('public.view_system_detail'::regclass, true);
    legacy_counts := substring(definition FROM
        'cve_counts AS \(.*?GROUP BY v\.hostname\s*\)');
    IF legacy_counts IS NULL
       OR position('view_system_vulnerabilities' IN legacy_counts) = 0 THEN
        RAISE EXCEPTION 'Unexpected view_system_detail CVE CTE; refusing partial rewrite';
    END IF;
    definition := replace(definition, legacy_counts, $counts$
        cve_counts AS (
            SELECT s.hostname,
                count(DISTINCT v.cve_id) FILTER (WHERE public.severity_from_cvss(c.cvss_v3_score) = 'CRITICAL')::integer AS critical_cve_count,
                count(DISTINCT v.cve_id) FILTER (WHERE public.severity_from_cvss(c.cvss_v3_score) = 'HIGH')::integer AS high_cve_count,
                count(DISTINCT v.cve_id) FILTER (WHERE public.severity_from_cvss(c.cvss_v3_score) = 'MEDIUM')::integer AS medium_cve_count,
                count(DISTINCT v.cve_id) FILTER (WHERE public.severity_from_cvss(c.cvss_v3_score) = 'LOW')::integer AS low_cve_count
            FROM public.view_current_exact_cve_occurrences v
            JOIN public.systems s ON s.id = v.system_id
            JOIN public.cves c ON c.id = v.cve_id
            WHERE s.is_active AND s.management_mode = 'managed'
            GROUP BY s.hostname
        )
    $counts$);
    IF position('view_system_vulnerabilities' IN definition) <> 0 THEN
        RAISE EXCEPTION 'Legacy CVE source remains in view_system_detail';
    END IF;
    EXECUTE 'CREATE OR REPLACE VIEW public.view_system_detail AS ' || definition;
END;
$migration$;

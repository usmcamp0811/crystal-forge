-- SECURITY: A persisted SCHEDULED row is effective only while its active POA&M
-- has complete reusable metadata, has an available typed assignee, and owns
-- exactly the current authoritative subjects in that environment. The two
-- NOT EXISTS clauses enforce bidirectional set equality. Historical actor and
-- assignment identity remain stored even when current coverage fails closed.
CREATE OR REPLACE FUNCTION cve_coherent_environment_disposition_state(
    v_cve_id text,
    v_package_name text,
    v_environment_id uuid
) RETURNS text LANGUAGE sql STABLE AS $$
SELECT CASE
  WHEN disposition.state='accepted' THEN 'accepted'
  WHEN disposition.state='scheduled'
   AND EXISTS (
     SELECT 1 FROM poams poam
     WHERE poam.id=disposition.poam_id
       AND poam.status<>'completed'
       AND poam.target_date IS NOT NULL
       AND (
         (poam.owner_kind='user'
          AND poam.owner_user_id IS NOT NULL
          AND poam.owner_group_name IS NULL
          AND EXISTS (
            SELECT 1 FROM users assignee
            WHERE assignee.id=poam.owner_user_id
              AND assignee.is_active
              AND assignee.user_type='human'))
         OR
         (poam.owner_kind='oidc_group'
          AND poam.owner_user_id IS NULL
          AND poam.owner_group_name IS NOT NULL
          AND btrim(poam.owner_group_name)<>''
          AND EXISTS (
            SELECT 1 FROM oidc_group_mappings mapping
            WHERE mapping.group_name=poam.owner_group_name))))
   AND NOT EXISTS (
     SELECT 1 FROM view_current_exact_cve_occurrences subject
     WHERE subject.cve_id=v_cve_id
       AND subject.package_name=v_package_name
       AND subject.environment_id=v_environment_id
       AND NOT EXISTS (
         SELECT 1 FROM poam_cve_finding_links link
         WHERE link.poam_id=disposition.poam_id
           AND link.system_id=subject.system_id
           AND link.canonical_cve_id=v_cve_id
           AND link.canonical_package_name=v_package_name
           AND link.retired_at IS NULL))
   AND NOT EXISTS (
     SELECT 1 FROM poam_cve_finding_links link
     JOIN systems linked_system ON linked_system.id=link.system_id
     WHERE link.poam_id=disposition.poam_id
       AND link.canonical_cve_id=v_cve_id
       AND link.canonical_package_name=v_package_name
       AND link.retired_at IS NULL
       AND linked_system.environment_id=v_environment_id
       AND NOT EXISTS (
         SELECT 1 FROM view_current_exact_cve_occurrences subject
         WHERE subject.system_id=link.system_id
           AND subject.cve_id=v_cve_id
           AND subject.package_name=v_package_name
           AND subject.environment_id=v_environment_id))
    THEN 'scheduled'
  ELSE NULL
END
FROM cve_current_environment_dispositions disposition
WHERE disposition.canonical_cve_id=v_cve_id
  AND disposition.canonical_package_name=v_package_name
  AND disposition.environment_id=v_environment_id
$$;

CREATE OR REPLACE FUNCTION cve_list_for_environment_scope(
    allowed_environment_ids uuid[]
) RETURNS TABLE (
  cve_id text,cvss_v3_score numeric,severity text,title text,
  cvss_vector text,published_date date,exploited boolean,
  package_name text,installed_version text,
  fixed_version text,fix_status text,affected_count bigint,
  affected_environments text[],first_seen timestamptz,last_seen timestamptz,
  age_days integer,triage_status text
) LANGUAGE sql STABLE AS $$
WITH scoped_occurrences AS (
  SELECT occurrence.*
  FROM view_current_exact_cve_occurrences occurrence
  WHERE allowed_environment_ids IS NULL
     OR occurrence.environment_id=ANY(allowed_environment_ids)
), package_metadata AS (
  SELECT occurrence.cve_id,occurrence.package_name,
         max(vulnerability.fixed_version) AS fixed_version
  FROM scoped_occurrences occurrence
  LEFT JOIN derivations package_derivation
    ON package_derivation.derivation_path=occurrence.observed_derivation_path
  LEFT JOIN package_vulnerabilities vulnerability
    ON vulnerability.derivation_id=package_derivation.id
   AND vulnerability.cve_id=occurrence.cve_id
  GROUP BY occurrence.cve_id,occurrence.package_name
), affected_environments AS (
  SELECT occurrence.cve_id,occurrence.package_name,
         occurrence.environment_id,occurrence.environment_name,
         cve_coherent_environment_disposition_state(
           occurrence.cve_id,occurrence.package_name,
           occurrence.environment_id) AS state
  FROM scoped_occurrences occurrence
  GROUP BY occurrence.cve_id,occurrence.package_name,
           occurrence.environment_id,occurrence.environment_name
), disposition_rollup AS (
  SELECT affected.cve_id,affected.package_name,
         CASE
           WHEN bool_and(COALESCE(affected.state='accepted',false))
             THEN 'accepted'
           WHEN bool_and(COALESCE(affected.state='scheduled',false))
             THEN 'scheduled'
           ELSE 'outstanding'
         END AS triage_status
  FROM affected_environments affected
  GROUP BY affected.cve_id,affected.package_name
)
SELECT cve.id,cve.cvss_v3_score,severity_from_cvss(cve.cvss_v3_score),
       COALESCE(NULLIF(btrim(cve.description),''),cve.id),cve.vector,
       cve.published_date,cve.exploited,occurrence.package_name,
       max(occurrence.installed_version),metadata.fixed_version::text,
       CASE WHEN metadata.fixed_version IS NULL
            THEN 'open' ELSE 'fix_available' END,
       count(DISTINCT occurrence.system_id),
       array_agg(DISTINCT occurrence.environment_name
                 ORDER BY occurrence.environment_name)
         FILTER (WHERE occurrence.environment_name IS NOT NULL),
       min(occurrence.completed_at),max(occurrence.completed_at),
       COALESCE(EXTRACT(EPOCH FROM (now()-cve.published_date))/86400,0)::integer,
       rollup.triage_status
FROM scoped_occurrences occurrence
JOIN cves cve ON cve.id=occurrence.cve_id
LEFT JOIN package_metadata metadata
  ON metadata.cve_id=occurrence.cve_id
 AND metadata.package_name=occurrence.package_name
JOIN disposition_rollup rollup
  ON rollup.cve_id=occurrence.cve_id
 AND rollup.package_name=occurrence.package_name
GROUP BY cve.id,cve.cvss_v3_score,cve.description,cve.vector,
         cve.published_date,cve.exploited,occurrence.package_name,
         metadata.fixed_version,rollup.triage_status
$$;

-- Drop leaf views before the base view, then recreate the base before leaves.
DROP VIEW view_cves_grouped_by_package;
DROP VIEW view_cve_fleet_stats;
DROP VIEW view_cve_list_with_metadata;

CREATE VIEW view_cve_list_with_metadata AS
SELECT * FROM cve_list_for_environment_scope(NULL);

CREATE VIEW view_cves_grouped_by_package AS
SELECT package_name,
       count(*) AS cve_count,
       count(*) FILTER (WHERE severity='CRITICAL') AS critical_count,
       count(*) FILTER (WHERE severity='HIGH') AS high_count,
       count(*) FILTER (WHERE severity='MEDIUM') AS medium_count,
       count(*) FILTER (WHERE severity='LOW') AS low_count,
       count(DISTINCT environment_name) AS environments_count,
       sum(affected_count) AS total_affected_systems,
       count(*) FILTER (WHERE fix_status='fix_available') AS fixable_count,
       count(*) FILTER (WHERE triage_status='outstanding') AS outstanding_count,
       count(*) FILTER (WHERE exploited) AS exploited_count,
       max(cvss_v3_score) AS max_cvss,
       sum(CASE severity WHEN 'CRITICAL' THEN 1000 WHEN 'HIGH' THEN 100
           WHEN 'MEDIUM' THEN 10 WHEN 'LOW' THEN 1 ELSE 0 END) AS severity_score
FROM view_cve_list_with_metadata
LEFT JOIN LATERAL unnest(affected_environments) environment_name ON true
WHERE package_name IS NOT NULL
GROUP BY package_name
ORDER BY severity_score DESC,max_cvss DESC NULLS LAST;

CREATE VIEW view_cve_fleet_stats AS
SELECT count(*) AS total_cves,
       count(*) FILTER (WHERE severity='CRITICAL') AS critical,
       count(*) FILTER (WHERE severity='HIGH') AS high,
       count(*) FILTER (WHERE severity='MEDIUM') AS medium,
       count(*) FILTER (WHERE severity='LOW') AS low,
       count(*) FILTER (WHERE exploited) AS exploited,
       count(*) FILTER (WHERE fix_status='fix_available') AS fixable,
       (SELECT count(DISTINCT environment_name)
        FROM view_cve_list_with_metadata scoped,
             unnest(scoped.affected_environments) environment_name)
         AS environments_affected,
       COALESCE(sum(affected_count),0) AS systems_affected,
       count(*) FILTER (WHERE triage_status='outstanding') AS outstanding,
       count(*) FILTER (WHERE triage_status='accepted') AS accepted,
       count(*) FILTER (WHERE triage_status='scheduled') AS scheduled
FROM view_cve_list_with_metadata;

COMMENT ON FUNCTION cve_coherent_environment_disposition_state(text,text,uuid) IS
  'Returns ACCEPTED directly. Returns SCHEDULED only for a non-completed POA&M with a target date, an available typed assignee, and active exact links set-equal to current authoritative subjects; otherwise returns NULL for OPEN.';
COMMENT ON VIEW view_cve_list_with_metadata IS
  'One current exact CVE/package row from the parameterized authority. OPEN or mixed state is conservatively OUTSTANDING.';
COMMENT ON FUNCTION cve_list_for_environment_scope(uuid[]) IS
  'Aggregates current exact CVE/package rows after caller environment scope and applies the same conservative scheduled POA&M/link coherence rule as the fleet drawer. NULL means Admin fleet-wide access; a non-NULL array excludes NULL and non-member environments.';

-- SECURITY: Roll up only affected environments visible to the caller. A
-- persisted SCHEDULED state is not effective until its current exact subjects
-- are covered; unassigned or hidden environments must not leak into a scope.
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
           WHEN bool_and(affected.environment_id IS NOT NULL
                         AND COALESCE(affected.state='accepted',false))
             THEN 'accepted'
           WHEN bool_and(affected.environment_id IS NOT NULL
                         AND COALESCE(affected.state='scheduled',false))
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

CREATE OR REPLACE VIEW view_cve_list_with_metadata AS
SELECT * FROM cve_list_for_environment_scope(NULL);

COMMENT ON FUNCTION cve_list_for_environment_scope(uuid[]) IS
  'Scopes exact occurrences before metadata and disposition rollup. NULL scope is Admin fleet-wide access; a concrete array excludes hidden and unassigned environments. SCHEDULED requires current POA&M coverage.';
COMMENT ON VIEW view_cve_list_with_metadata IS
  'Fleet-wide exact CVE/package rows. SCHEDULED requires coherent current environment coverage; OPEN or mixed environment states roll up to OUTSTANDING.';

-- SECURITY: The 0279 trigger validates fresh baselines and takes CVE, system,
-- then finding locks before this guard runs. Its identical-history exception
-- must not permit a retired non-closure link to bypass Current proof. Explicit
-- closure reopen retains its immutable baseline after the POA&M is reopened.
CREATE FUNCTION guard_poam_cve_finding_link_replay()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
DECLARE v_nonclosure_replay boolean;
BEGIN
    SELECT bool_or(attempt.id IS NULL)
      INTO v_nonclosure_replay
    FROM poam_cve_finding_links history
    LEFT JOIN poam_verification_attempts attempt
      ON history.retirement_reason='closed:'||attempt.id::text
     AND attempt.poam_id=history.poam_id
     AND attempt.outcome='accepted' AND attempt.sealed_at IS NOT NULL
    WHERE history.poam_id=NEW.poam_id
          AND history.cve_finding_id=NEW.cve_finding_id
          AND history.system_id=NEW.system_id
          AND history.canonical_cve_id=NEW.canonical_cve_id
          AND history.canonical_package_name=NEW.canonical_package_name
          AND history.baseline_scan_id=NEW.baseline_scan_id
          AND history.baseline_scan_derivation_id=NEW.baseline_scan_derivation_id
          AND history.baseline_scan_completed_at=NEW.baseline_scan_completed_at
          AND history.baseline_generation_snapshot_id IS NOT DISTINCT FROM
              NEW.baseline_generation_snapshot_id
          AND history.baseline_generation=NEW.baseline_generation
          AND history.baseline_target_store_path=NEW.baseline_target_store_path
          AND history.baseline_occurrence_derivation_path=
              NEW.baseline_occurrence_derivation_path
          AND history.baseline_observed_package_version=
              NEW.baseline_observed_package_version
          AND history.retired_at IS NOT NULL;
    IF v_nonclosure_replay IS NULL THEN RETURN NEW; END IF;

    IF EXISTS (
        SELECT 1 FROM cve_current_system_dispositions host
        WHERE host.system_id=NEW.system_id
          AND host.canonical_cve_id=NEW.canonical_cve_id
          AND host.canonical_package_name=NEW.canonical_package_name
          AND host.state='accepted'
    ) THEN
        RAISE EXCEPTION 'Accepted host risk must be retired before CVE link replay'
            USING ERRCODE='23514',
                  CONSTRAINT='poam_cve_link_replay_accepted_host_risk';
    END IF;

    IF v_nonclosure_replay AND NOT EXISTS (
            SELECT 1 FROM view_current_cve_authority authority
            JOIN cve_scan_vulnerability_observations observation
              ON observation.scan_id=authority.scan_id
             AND observation.observed_derivation_path=
                 NEW.baseline_occurrence_derivation_path
             AND observation.canonical_cve_id=NEW.canonical_cve_id
             AND observation.canonical_package_name=NEW.canonical_package_name
             AND observation.observed_package_version=
                 NEW.baseline_observed_package_version
             AND NOT observation.is_whitelisted
            WHERE authority.system_id=NEW.system_id
              AND authority.generation=NEW.baseline_generation
              AND authority.store_path=NEW.baseline_target_store_path
              AND authority.derivation_id=NEW.baseline_scan_derivation_id
              AND authority.scan_id=NEW.baseline_scan_id
              AND authority.scan_completed_at=NEW.baseline_scan_completed_at
              AND (NEW.baseline_generation_snapshot_id IS NULL
                   OR authority.generation_snapshot_id=
                      NEW.baseline_generation_snapshot_id)
              AND NOT EXISTS (
                  SELECT 1 FROM system_cve_justifications justification
                  WHERE justification.cve_id=NEW.canonical_cve_id
                    AND (justification.system_id IS NULL
                         OR justification.system_id=NEW.system_id))
    ) THEN
        RAISE EXCEPTION 'Retired non-closure CVE link requires current exact baseline'
            USING ERRCODE='23514',
                  CONSTRAINT='poam_cve_link_replay_current_baseline';
    END IF;
    RETURN NEW;
END;
$$;

-- PostgreSQL fires same-kind triggers by name. Run after 0279's protection
-- trigger so its established lock order and completed-POA&M check still apply.
CREATE TRIGGER trigger_z_guard_poam_cve_finding_link_replay
    BEFORE INSERT ON poam_cve_finding_links
    FOR EACH ROW EXECUTE FUNCTION guard_poam_cve_finding_link_replay();

COMMENT ON FUNCTION guard_poam_cve_finding_link_replay() IS
  'Rejects retired-link replay under active accepted host risk. A non-closure baseline must still match the exact authorized Current occurrence; closed-attempt history may be reopened after POA&M status restoration.';

-- Exact CVE evidence is opt-in. Existing scan rows remain schema version 0 and
-- are not inferred from mutable package_vulnerabilities state.
ALTER TABLE cve_scans
    ADD COLUMN evidence_schema_version integer NOT NULL DEFAULT 0,
    ADD CONSTRAINT cve_scans_evidence_schema_version_valid CHECK (
        evidence_schema_version IN (0, 1)
        AND (evidence_schema_version = 0
            OR (status = 'completed' AND completed_at IS NOT NULL))
    );

ALTER TABLE cve_scans
    ADD CONSTRAINT cve_scans_exact_evidence_source_unique
        UNIQUE (id, derivation_id, completed_at);

ALTER TABLE derivations
    ADD CONSTRAINT derivations_id_derivation_path_unique
        UNIQUE (id, derivation_path);

-- INVARIANT: Version 1 is a terminal evidence seal. New rows must start at
-- version 0, and the only transition to version 1 completes an active scan.
-- Every field is immutable after that transition because counts, completion
-- time, scanner metadata, and derivation identity are evidence provenance.
CREATE FUNCTION protect_cve_scan_exact_evidence()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP = 'INSERT' THEN
        IF NEW.evidence_schema_version <> 0 THEN
            RAISE EXCEPTION 'A CVE scan must begin at evidence schema version 0'
                USING ERRCODE = '23514',
                      CONSTRAINT = 'cve_scans_initial_evidence_version';
        END IF;
        RETURN NEW;
    END IF;
    IF TG_OP = 'DELETE' THEN RETURN OLD; END IF;
    IF OLD.evidence_schema_version = 1 AND NEW IS DISTINCT FROM OLD THEN
        RAISE EXCEPTION 'Completed exact CVE scan evidence is immutable';
    END IF;
    IF OLD.evidence_schema_version = 0
       AND NEW.evidence_schema_version = 1
       AND NOT (OLD.status = 'in_progress' AND NEW.status = 'completed') THEN
        RAISE EXCEPTION 'Exact CVE evidence version requires atomic scan completion'
            USING ERRCODE = '23514',
                  CONSTRAINT = 'cve_scans_atomic_evidence_completion';
    END IF;
    RETURN NEW;
END;
$$;
CREATE TRIGGER trigger_protect_cve_scan_exact_evidence
    BEFORE INSERT OR UPDATE OR DELETE ON cve_scans
    FOR EACH ROW EXECUTE FUNCTION protect_cve_scan_exact_evidence();

CREATE TABLE cve_scan_vulnerability_observations (
    scan_id uuid NOT NULL REFERENCES cve_scans(id) ON DELETE CASCADE,
    canonical_cve_id varchar(20) NOT NULL REFERENCES cves(id) ON DELETE RESTRICT,
    canonical_package_name text NOT NULL,
    observed_package_name text NOT NULL,
    observed_package_version text NOT NULL,
    observed_derivation_path text NOT NULL,
    is_whitelisted boolean NOT NULL,
    whitelist_reason text,
    detection_method text NOT NULL,
    observed_at timestamptz NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (
        scan_id, observed_derivation_path, canonical_cve_id,
        canonical_package_name
    ),
    CHECK (canonical_cve_id ~ '^CVE-[0-9]{4}-[0-9]{4,}$'),
    CHECK (canonical_package_name = btrim(canonical_package_name)
        AND canonical_package_name <> ''),
    CHECK (btrim(observed_package_name) <> ''),
    CHECK (btrim(observed_derivation_path) <> ''),
    CHECK (btrim(detection_method) <> ''),
    CHECK ((NOT is_whitelisted AND whitelist_reason IS NULL)
        OR (is_whitelisted AND btrim(COALESCE(whitelist_reason, '')) <> ''))
);

-- CONCURRENCY: Taking the parent row lock serializes append with completion.
-- An append that wins is included before the seal. An append that waits for a
-- completed scan observes version 1 and fails. Observation rows never mutate.
CREATE FUNCTION protect_cve_scan_vulnerability_observation()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
DECLARE
    v_status text;
    v_version integer;
BEGIN
    IF TG_OP = 'DELETE' AND NOT EXISTS (
        SELECT 1 FROM cve_scans WHERE id=OLD.scan_id
    ) THEN
        RETURN OLD;
    END IF;
    IF TG_OP <> 'INSERT' THEN
        RAISE EXCEPTION 'CVE scan observations are immutable';
    END IF;
    SELECT status, evidence_schema_version INTO v_status, v_version
    FROM cve_scans
    WHERE id = NEW.scan_id
    FOR UPDATE;
    IF NOT FOUND OR v_status <> 'in_progress' OR v_version <> 0 THEN
        RAISE EXCEPTION 'CVE observations require an active unsealed scan'
            USING ERRCODE = '23514',
                  CONSTRAINT = 'cve_observations_active_unsealed_scan';
    END IF;
    RETURN NEW;
END;
$$;
CREATE TRIGGER trigger_protect_cve_scan_vulnerability_observation
    BEFORE INSERT OR UPDATE OR DELETE ON cve_scan_vulnerability_observations
    FOR EACH ROW EXECUTE FUNCTION protect_cve_scan_vulnerability_observation();

COMMENT ON TABLE cve_scan_vulnerability_observations IS
    'Immutable scanner occurrences. Only evidence_schema_version 1 scans are authoritative; legacy version 0 scans are never inferred.';
COMMENT ON COLUMN cve_scan_vulnerability_observations.observed_package_version IS
    'Exact scanner version text. An empty value is retained when Vulnix reports an unknown version.';

-- CVE findings use package pname, not installed version, as stable identity.
-- Installed versions and derivation paths belong to immutable scan evidence.
CREATE TABLE poam_cve_findings (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    system_id uuid NOT NULL REFERENCES systems(id) ON DELETE RESTRICT,
    canonical_cve_id varchar(20) NOT NULL REFERENCES cves(id) ON DELETE RESTRICT,
    canonical_package_name text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE (system_id, canonical_cve_id, canonical_package_name),
    UNIQUE (id, system_id, canonical_cve_id, canonical_package_name),
    CHECK (canonical_cve_id ~ '^CVE-[0-9]{4}-[0-9]{4,}$'),
    CHECK (canonical_package_name = btrim(canonical_package_name)
        AND canonical_package_name <> '')
);

-- A disposition applies to one durable environment scope. OPEN is represented
-- by the absence of an active row so accepted risk cannot be confused with a
-- remediation result or a successful verification item.
CREATE TABLE cve_environment_dispositions (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    canonical_cve_id varchar(20) NOT NULL REFERENCES cves(id) ON DELETE RESTRICT,
    canonical_package_name text NOT NULL,
    environment_id uuid NOT NULL REFERENCES environments(id) ON DELETE RESTRICT,
    state text NOT NULL CHECK (state IN ('accepted', 'scheduled')),
    justification text,
    review_date date,
    accepted_by uuid REFERENCES users(id) ON DELETE RESTRICT,
    accepted_at timestamptz,
    poam_id uuid REFERENCES poams(id) ON DELETE RESTRICT,
    scheduled_by uuid REFERENCES users(id) ON DELETE RESTRICT,
    scheduled_at timestamptz,
    retired_at timestamptz,
    retired_by uuid REFERENCES users(id) ON DELETE RESTRICT,
    retirement_reason text,
    CHECK (canonical_cve_id ~ '^CVE-[0-9]{4}-[0-9]{4,}$'),
    CHECK (canonical_package_name=btrim(canonical_package_name)
        AND canonical_package_name<>''),
    CHECK (
      (state='accepted'
        AND btrim(COALESCE(justification,''))<>''
        AND accepted_by IS NOT NULL AND accepted_at IS NOT NULL
        AND poam_id IS NULL AND scheduled_by IS NULL AND scheduled_at IS NULL)
      OR
      (state='scheduled'
        AND justification IS NULL AND review_date IS NULL
        AND accepted_by IS NULL AND accepted_at IS NULL
        AND poam_id IS NOT NULL
        AND scheduled_by IS NOT NULL AND scheduled_at IS NOT NULL)
    ),
    CHECK ((retired_at IS NULL AND retired_by IS NULL
            AND retirement_reason IS NULL)
        OR (retired_at IS NOT NULL AND retired_by IS NOT NULL
            AND btrim(COALESCE(retirement_reason,''))<>'')),
    UNIQUE (id, canonical_cve_id, canonical_package_name, environment_id)
);

CREATE UNIQUE INDEX cve_environment_dispositions_one_active
    ON cve_environment_dispositions(
      canonical_cve_id,canonical_package_name,environment_id)
    WHERE retired_at IS NULL;
CREATE INDEX cve_environment_dispositions_history
    ON cve_environment_dispositions(
      canonical_cve_id,canonical_package_name,environment_id,
      accepted_at DESC,scheduled_at DESC,id DESC);

-- INVARIANT: Disposition history is append-only. The only permitted update
-- retires an active row, and a retired row can never change or be deleted.
CREATE FUNCTION protect_cve_environment_disposition_history()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP='DELETE' THEN
        RAISE EXCEPTION 'CVE environment disposition history is immutable';
    END IF;
    IF TG_OP='INSERT' THEN RETURN NEW; END IF;
    IF OLD.retired_at IS NOT NULL OR NEW.id<>OLD.id
       OR NEW.canonical_cve_id<>OLD.canonical_cve_id
       OR NEW.canonical_package_name<>OLD.canonical_package_name
       OR NEW.environment_id<>OLD.environment_id
       OR NEW.state<>OLD.state
       OR NEW.justification IS DISTINCT FROM OLD.justification
       OR NEW.review_date IS DISTINCT FROM OLD.review_date
       OR NEW.accepted_by IS DISTINCT FROM OLD.accepted_by
       OR NEW.accepted_at IS DISTINCT FROM OLD.accepted_at
       OR NEW.poam_id IS DISTINCT FROM OLD.poam_id
       OR NEW.scheduled_by IS DISTINCT FROM OLD.scheduled_by
       OR NEW.scheduled_at IS DISTINCT FROM OLD.scheduled_at
       OR NEW.retired_at IS NULL OR NEW.retired_by IS NULL
       OR btrim(COALESCE(NEW.retirement_reason,''))='' THEN
        RAISE EXCEPTION 'A CVE environment disposition update must retire the active row';
    END IF;
    RETURN NEW;
END;
$$;
CREATE TRIGGER trigger_protect_cve_environment_disposition_history
    BEFORE INSERT OR UPDATE OR DELETE ON cve_environment_dispositions
    FOR EACH ROW EXECUTE FUNCTION protect_cve_environment_disposition_history();

CREATE VIEW cve_current_environment_dispositions AS
SELECT * FROM cve_environment_dispositions WHERE retired_at IS NULL;

COMMENT ON TABLE cve_environment_dispositions IS
    'Append-only accepted-risk or scheduled-remediation state for one canonical CVE, package, and environment. No active row means OPEN.';
COMMENT ON COLUMN cve_environment_dispositions.poam_id IS
    'The one remediation POA&M for a scheduled environment. Accepted risk never references a POA&M.';

-- PERSISTENCE: Global CVE list, grouped, export, filter, and statistics reads
-- use the same retained deployed-generation schema-1 occurrence authority as
-- exact fleet triage. Mutable legacy package rows and fleet justifications do
-- not establish current occurrence or disposition state.
CREATE VIEW view_current_exact_cve_occurrences AS
SELECT DISTINCT ON (
         system.id,observation.canonical_cve_id,
         observation.canonical_package_name)
       system.id AS system_id,system.environment_id,
       environment.name AS environment_name,
       scan.id AS scan_id,scan.completed_at,
       observation.canonical_cve_id AS cve_id,
       observation.canonical_package_name AS package_name,
       observation.observed_package_version AS installed_version,
       observation.observed_derivation_path
FROM systems system
LEFT JOIN environments environment ON environment.id=system.environment_id
JOIN LATERAL (
  SELECT state.store_path,state.generation
  FROM system_states state
  WHERE state.hostname=system.hostname
    AND state.store_path IS NOT NULL
    AND state.generation IS NOT NULL
    AND state.generation_matches_current_store_path IS TRUE
    AND btrim(state.store_path)<>''
  ORDER BY state.timestamp DESC,state.id DESC LIMIT 1
) deployed ON true
JOIN evaluation_generation_snapshots retained
  ON retained.system_id=system.id
 AND retained.generation=deployed.generation
 AND retained.source_store_path=deployed.store_path
 AND retained.lineage_verified
JOIN evaluation_snapshots artifact
  ON artifact.id=retained.snapshot_id
 AND artifact.commit_id=retained.commit_id
 AND artifact.configuration_name=retained.configuration_name
 AND artifact.lifecycle='available' AND artifact.integrity_version=1
JOIN derivations derivation
  ON derivation.id=retained.derivation_id
 AND derivation.commit_id=retained.commit_id
 AND derivation.derivation_name=retained.configuration_name
 AND derivation.derivation_type='nixos'
 AND COALESCE(derivation.store_path,derivation.expected_store_path)
     =retained.source_store_path
JOIN LATERAL (
  SELECT candidate.id,candidate.completed_at
  FROM cve_scans candidate
  WHERE candidate.derivation_id=derivation.id
    AND candidate.status='completed'
    AND candidate.completed_at IS NOT NULL
    AND candidate.evidence_schema_version=1
  ORDER BY candidate.completed_at DESC,candidate.id DESC LIMIT 1
) scan ON true
JOIN cve_scan_vulnerability_observations observation
  ON observation.scan_id=scan.id AND NOT observation.is_whitelisted
WHERE system.is_active
ORDER BY system.id,observation.canonical_cve_id,
         observation.canonical_package_name,
         observation.observed_derivation_path;

-- PostgreSQL cannot change a view column from varchar to text in place. Drop
-- the two dependent dashboard views first, then rebuild all three with exact
-- text identities.
DROP VIEW view_cves_grouped_by_package;
DROP VIEW view_cve_fleet_stats;
DROP VIEW view_cve_list_with_metadata;

CREATE VIEW view_cve_list_with_metadata AS
WITH package_metadata AS (
  SELECT occurrence.cve_id,occurrence.package_name,
         max(vulnerability.fixed_version) AS fixed_version
  FROM view_current_exact_cve_occurrences occurrence
  LEFT JOIN derivations package_derivation
    ON package_derivation.derivation_path=occurrence.observed_derivation_path
  LEFT JOIN package_vulnerabilities vulnerability
    ON vulnerability.derivation_id=package_derivation.id
   AND vulnerability.cve_id=occurrence.cve_id
  GROUP BY occurrence.cve_id,occurrence.package_name
), affected_environments AS (
  SELECT occurrence.cve_id,occurrence.package_name,
         occurrence.environment_id,occurrence.environment_name,
         disposition.state
  FROM view_current_exact_cve_occurrences occurrence
  LEFT JOIN cve_current_environment_dispositions disposition
    ON disposition.canonical_cve_id=occurrence.cve_id
   AND disposition.canonical_package_name=occurrence.package_name
   AND disposition.environment_id=occurrence.environment_id
  GROUP BY occurrence.cve_id,occurrence.package_name,
           occurrence.environment_id,occurrence.environment_name,
           disposition.state
), disposition_rollup AS (
  SELECT cve_id,package_name,
         CASE
            WHEN bool_and(environment_id IS NOT NULL
                          AND COALESCE(state='accepted',false))
              THEN 'accepted'
            WHEN bool_and(environment_id IS NOT NULL
                          AND COALESCE(state='scheduled',false))
             THEN 'scheduled'
           ELSE 'outstanding'
         END AS triage_status
  FROM affected_environments
  GROUP BY cve_id,package_name
)
SELECT cve.id AS cve_id,cve.cvss_v3_score,
       severity_from_cvss(cve.cvss_v3_score) AS severity,
       COALESCE(NULLIF(btrim(cve.description),''),cve.id) AS title,
       cve.vector AS cvss_vector,cve.published_date,cve.exploited,
       occurrence.package_name AS package_name,
       max(occurrence.installed_version) AS installed_version,
       metadata.fixed_version::text AS fixed_version,
       CASE WHEN metadata.fixed_version IS NULL
            THEN 'open' ELSE 'fix_available' END AS fix_status,
       count(DISTINCT occurrence.system_id) AS affected_count,
       array_agg(DISTINCT occurrence.environment_name
                 ORDER BY occurrence.environment_name)
         FILTER (WHERE occurrence.environment_name IS NOT NULL)
         AS affected_environments,
       min(occurrence.completed_at) AS first_seen,
       max(occurrence.completed_at) AS last_seen,
       COALESCE(EXTRACT(EPOCH FROM (now()-cve.published_date))/86400,0)::integer
         AS age_days,
       rollup.triage_status
FROM view_current_exact_cve_occurrences occurrence
JOIN cves cve ON cve.id=occurrence.cve_id
LEFT JOIN package_metadata metadata
  ON metadata.cve_id=occurrence.cve_id
 AND metadata.package_name=occurrence.package_name
JOIN disposition_rollup rollup
  ON rollup.cve_id=occurrence.cve_id
 AND rollup.package_name=occurrence.package_name
GROUP BY cve.id,cve.cvss_v3_score,cve.description,cve.vector,
         cve.published_date,cve.exploited,occurrence.package_name,
         metadata.fixed_version,rollup.triage_status;

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

COMMENT ON VIEW view_current_exact_cve_occurrences IS
  'Current non-whitelisted exact CVE/package occurrences from the latest sealed schema-1 scan for each retained deployed generation.';
COMMENT ON VIEW view_cve_list_with_metadata IS
  'One current exact CVE/package row. ACCEPTED and SCHEDULED require every affected environment to have that state; OPEN or mixed environment state is conservatively OUTSTANDING.';
COMMENT ON VIEW view_cves_grouped_by_package IS
  'Groups current exact CVE/package rows without narrowing package identity text.';
COMMENT ON VIEW view_cve_fleet_stats IS
  'Aggregates fleet statistics from current exact CVE/package rows.';

-- SECURITY: Scope authoritative occurrences before aggregation. A NULL scope
-- is reserved for Admin callers. A concrete array, including an empty array,
-- excludes unassigned and unauthorized environments without exposing global
-- package, CVE, status, environment, or count information.
CREATE FUNCTION cve_list_for_environment_scope(allowed_environment_ids uuid[])
RETURNS TABLE (
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
         disposition.state
  FROM scoped_occurrences occurrence
  LEFT JOIN cve_current_environment_dispositions disposition
    ON disposition.canonical_cve_id=occurrence.cve_id
   AND disposition.canonical_package_name=occurrence.package_name
   AND disposition.environment_id=occurrence.environment_id
  GROUP BY occurrence.cve_id,occurrence.package_name,
           occurrence.environment_id,occurrence.environment_name,
           disposition.state
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
       cve.published_date,cve.exploited,
       occurrence.package_name,
       max(occurrence.installed_version),
       metadata.fixed_version::text,
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

COMMENT ON FUNCTION cve_list_for_environment_scope(uuid[]) IS
  'Aggregates current exact CVE/package rows only after applying the caller environment scope. NULL means Admin fleet-wide access; a non-NULL array excludes NULL and non-member environments.';

CREATE FUNCTION prevent_poam_cve_finding_identity_mutation()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    IF NEW.system_id <> OLD.system_id
       OR NEW.canonical_cve_id <> OLD.canonical_cve_id
       OR NEW.canonical_package_name <> OLD.canonical_package_name THEN
        RAISE EXCEPTION 'CVE POA&M finding identity is immutable';
    END IF;
    RETURN NEW;
END;
$$;
CREATE TRIGGER trigger_prevent_poam_cve_finding_identity_mutation
    BEFORE UPDATE OF system_id, canonical_cve_id, canonical_package_name
    ON poam_cve_findings
    FOR EACH ROW EXECUTE FUNCTION prevent_poam_cve_finding_identity_mutation();

CREATE TABLE poam_cve_finding_links (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    poam_id uuid NOT NULL REFERENCES poams(id) ON DELETE CASCADE,
    cve_finding_id uuid NOT NULL,
    system_id uuid NOT NULL,
    canonical_cve_id varchar(20) NOT NULL,
    canonical_package_name text NOT NULL,
    baseline_scan_id uuid NOT NULL,
    baseline_scan_derivation_id integer NOT NULL,
    baseline_scan_completed_at timestamptz NOT NULL,
    baseline_generation_snapshot_id uuid NOT NULL
        REFERENCES evaluation_generation_snapshots(id) ON DELETE RESTRICT,
    baseline_generation integer NOT NULL CHECK (baseline_generation >= 0),
    baseline_target_store_path text NOT NULL
        CHECK (btrim(baseline_target_store_path) <> ''),
    baseline_occurrence_derivation_path text NOT NULL
        CHECK (btrim(baseline_occurrence_derivation_path) <> ''),
    baseline_observed_package_version text NOT NULL,
    linked_by uuid NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    linked_at timestamptz NOT NULL DEFAULT CURRENT_TIMESTAMP,
    retired_at timestamptz,
    retired_by uuid REFERENCES users(id) ON DELETE RESTRICT,
    retirement_reason text,
    CHECK ((retired_at IS NULL) = (retired_by IS NULL)),
    CHECK (retired_at IS NULL OR btrim(retirement_reason) <> ''),
    FOREIGN KEY (
        cve_finding_id,
        system_id,
        canonical_cve_id,
        canonical_package_name
    ) REFERENCES poam_cve_findings(
        id,
        system_id,
        canonical_cve_id,
        canonical_package_name
    ) ON DELETE RESTRICT,
    FOREIGN KEY (
        baseline_scan_id,
        baseline_scan_derivation_id,
        baseline_scan_completed_at
    ) REFERENCES cve_scans(id, derivation_id, completed_at) ON DELETE RESTRICT,
    FOREIGN KEY (
        baseline_scan_id, baseline_occurrence_derivation_path,
        canonical_cve_id, canonical_package_name
    ) REFERENCES cve_scan_vulnerability_observations(
        scan_id, observed_derivation_path, canonical_cve_id,
        canonical_package_name
    ) ON DELETE RESTRICT
);

COMMENT ON TABLE poam_cve_finding_links IS
    'Append-only exact-CVE remediation links. Every active and retired row retains the authoritative link-time scan, occurrence, and deployed-generation lineage used as its verification baseline.';

CREATE UNIQUE INDEX poam_cve_finding_links_one_active_remediation
    ON poam_cve_finding_links(cve_finding_id) WHERE retired_at IS NULL;
CREATE UNIQUE INDEX poam_cve_finding_links_one_active_pair
    ON poam_cve_finding_links(poam_id, cve_finding_id) WHERE retired_at IS NULL;
CREATE INDEX poam_cve_finding_links_history_order_idx
    ON poam_cve_finding_links(cve_finding_id, linked_at DESC, id DESC)
    INCLUDE(poam_id, retired_at);

-- CONCURRENCY: The namespace and complete canonical identity make this lock
-- independent from policy lineage locks. Every caller takes the global CVE key,
-- system sentinel, policy keys, then exact-CVE keys in lexical order.
CREATE FUNCTION lock_poam_cve_key(v_cve_id text)
RETURNS void LANGUAGE sql AS $$
    SELECT pg_advisory_xact_lock(hashtextextended(v_cve_id, 439));
$$;

CREATE FUNCTION try_lock_poam_cve_key(v_cve_id text)
RETURNS void LANGUAGE plpgsql AS $$
BEGIN
    IF NOT pg_try_advisory_xact_lock(hashtextextended(v_cve_id, 439)) THEN
        RAISE EXCEPTION 'POA&M CVE state changed concurrently'
            USING ERRCODE='40001';
    END IF;
END;
$$;

CREATE FUNCTION lock_poam_cve_finding_key(
    v_system_id uuid,
    v_cve_id text,
    v_package_name text
) RETURNS void LANGUAGE sql AS $$
    SELECT pg_advisory_xact_lock(hashtextextended(
        v_system_id::text || ':' || v_cve_id || ':' || v_package_name, 440));
$$;

CREATE FUNCTION try_lock_poam_cve_finding_key(
    v_system_id uuid,
    v_cve_id text,
    v_package_name text
) RETURNS void LANGUAGE plpgsql AS $$
BEGIN
    IF NOT pg_try_advisory_xact_lock(hashtextextended(
        v_system_id::text || ':' || v_cve_id || ':' || v_package_name, 440)) THEN
        RAISE EXCEPTION 'POA&M exact-CVE finding state changed concurrently'
            USING ERRCODE='40001';
    END IF;
END;
$$;

CREATE FUNCTION protect_poam_cve_finding_link_history()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP = 'INSERT' THEN
        PERFORM lock_poam_cve_key(NEW.canonical_cve_id);
        PERFORM lock_poam_finding_key(NEW.system_id, '00000000-0000-0000-0000-000000000000');
        PERFORM lock_poam_cve_finding_key(
            NEW.system_id, NEW.canonical_cve_id, NEW.canonical_package_name);
        IF EXISTS (SELECT 1 FROM poams WHERE id = NEW.poam_id AND status = 'completed') THEN
            RAISE EXCEPTION 'A completed POA&M cannot accept a CVE finding link'
                USING ERRCODE = '23514',
                      CONSTRAINT = 'poams_completed_without_cve_history_additions';
        END IF;
        -- SECURITY: The database accepts only the exact latest server-resolved
        -- occurrence for the retained deployment. Supplying another scan,
        -- generation, derivation, store path, version, or disposition fails.
        IF NOT EXISTS (
            SELECT 1
            FROM systems system
            JOIN LATERAL (
              SELECT state.store_path,state.generation
              FROM system_states state
              WHERE state.hostname=system.hostname
                AND state.store_path IS NOT NULL
                AND state.generation IS NOT NULL
                AND state.generation_matches_current_store_path IS TRUE
                AND btrim(state.store_path)<>''
              ORDER BY state.timestamp DESC,state.id DESC LIMIT 1
            ) deployed ON true
            JOIN evaluation_generation_snapshots retained
              ON retained.id=NEW.baseline_generation_snapshot_id
             AND retained.system_id=system.id
             AND retained.generation=NEW.baseline_generation
             AND retained.source_store_path=NEW.baseline_target_store_path
             AND retained.source_store_path=deployed.store_path
             AND retained.generation=deployed.generation
             AND retained.derivation_id=NEW.baseline_scan_derivation_id
             AND retained.lineage_verified
            JOIN evaluation_snapshots artifact
              ON artifact.id=retained.snapshot_id
             AND artifact.commit_id=retained.commit_id
             AND artifact.configuration_name=retained.configuration_name
             AND artifact.lifecycle='available' AND artifact.integrity_version=1
            JOIN derivations derivation
              ON derivation.id=retained.derivation_id
             AND derivation.commit_id=retained.commit_id
             AND derivation.derivation_name=retained.configuration_name
             AND derivation.derivation_type='nixos'
             AND COALESCE(derivation.store_path,derivation.expected_store_path)
                 =retained.source_store_path
            JOIN cve_scans scan
              ON scan.id=NEW.baseline_scan_id
             AND scan.derivation_id=NEW.baseline_scan_derivation_id
             AND scan.completed_at=NEW.baseline_scan_completed_at
             AND scan.status='completed' AND scan.evidence_schema_version=1
            JOIN cve_scan_vulnerability_observations observation
              ON observation.scan_id=scan.id
             AND observation.observed_derivation_path=
                 NEW.baseline_occurrence_derivation_path
             AND observation.canonical_cve_id=NEW.canonical_cve_id
             AND observation.canonical_package_name=NEW.canonical_package_name
             AND observation.observed_package_version=
                 NEW.baseline_observed_package_version
             AND NOT observation.is_whitelisted
            WHERE system.id=NEW.system_id
              AND scan.id=(SELECT latest.id FROM cve_scans latest
                WHERE latest.derivation_id=derivation.id
                  AND latest.status='completed'
                  AND latest.evidence_schema_version=1
                ORDER BY latest.completed_at DESC,latest.id DESC LIMIT 1)
              AND NOT EXISTS (
                SELECT 1 FROM system_cve_justifications justification
                WHERE justification.cve_id=NEW.canonical_cve_id
                  AND (justification.system_id IS NULL
                    OR justification.system_id=NEW.system_id))
        ) AND NOT EXISTS (
            SELECT 1 FROM poam_cve_finding_links history
            WHERE history.poam_id=NEW.poam_id
              AND history.cve_finding_id=NEW.cve_finding_id
              AND history.system_id=NEW.system_id
              AND history.canonical_cve_id=NEW.canonical_cve_id
              AND history.canonical_package_name=NEW.canonical_package_name
              AND history.baseline_scan_id=NEW.baseline_scan_id
              AND history.baseline_scan_derivation_id=
                  NEW.baseline_scan_derivation_id
              AND history.baseline_scan_completed_at=
                  NEW.baseline_scan_completed_at
              AND history.baseline_generation_snapshot_id=
                  NEW.baseline_generation_snapshot_id
              AND history.baseline_generation=NEW.baseline_generation
              AND history.baseline_target_store_path=
                  NEW.baseline_target_store_path
              AND history.baseline_occurrence_derivation_path=
                  NEW.baseline_occurrence_derivation_path
              AND history.baseline_observed_package_version=
                  NEW.baseline_observed_package_version
              AND history.retired_at IS NOT NULL
        ) THEN
            RAISE EXCEPTION 'CVE link baseline must match the current authoritative occurrence'
                USING ERRCODE='23514',
                      CONSTRAINT='poam_cve_link_authoritative_baseline';
        END IF;
        RETURN NEW;
    END IF;
    IF TG_OP = 'DELETE' THEN
        RAISE EXCEPTION 'CVE POA&M finding link history is immutable';
    END IF;
    IF NEW.id <> OLD.id OR NEW.poam_id <> OLD.poam_id
       OR NEW.cve_finding_id <> OLD.cve_finding_id
       OR NEW.system_id <> OLD.system_id
       OR NEW.canonical_cve_id <> OLD.canonical_cve_id
       OR NEW.canonical_package_name <> OLD.canonical_package_name
       OR NEW.baseline_scan_id <> OLD.baseline_scan_id
       OR NEW.baseline_scan_derivation_id <> OLD.baseline_scan_derivation_id
       OR NEW.baseline_scan_completed_at <> OLD.baseline_scan_completed_at
       OR NEW.baseline_generation_snapshot_id <> OLD.baseline_generation_snapshot_id
       OR NEW.baseline_generation <> OLD.baseline_generation
       OR NEW.baseline_target_store_path <> OLD.baseline_target_store_path
       OR NEW.baseline_occurrence_derivation_path <>
          OLD.baseline_occurrence_derivation_path
       OR NEW.baseline_observed_package_version <>
          OLD.baseline_observed_package_version
       OR NEW.linked_by <> OLD.linked_by OR NEW.linked_at <> OLD.linked_at THEN
        RAISE EXCEPTION 'CVE POA&M finding link identity is immutable';
    END IF;
    IF OLD.retired_at IS NOT NULL THEN
        RAISE EXCEPTION 'Retired CVE POA&M finding links are immutable';
    END IF;
    IF NEW.retired_at IS NULL OR NEW.retired_by IS NULL
       OR btrim(COALESCE(NEW.retirement_reason, '')) = '' THEN
        RAISE EXCEPTION 'A CVE POA&M finding link update must retire the active link';
    END IF;
    RETURN NEW;
END;
$$;
CREATE TRIGGER trigger_protect_poam_cve_finding_link_history
    BEFORE INSERT OR UPDATE OR DELETE ON poam_cve_finding_links
    FOR EACH ROW EXECUTE FUNCTION protect_poam_cve_finding_link_history();

CREATE TABLE poam_cve_verification_items (
    attempt_id uuid NOT NULL REFERENCES poam_verification_attempts(id) ON DELETE RESTRICT,
    cve_finding_id uuid NOT NULL,
    system_id uuid NOT NULL,
    canonical_cve_id varchar(20) NOT NULL,
    canonical_package_name text NOT NULL,
    baseline_scan_id uuid NOT NULL,
    baseline_scan_derivation_id integer NOT NULL,
    baseline_scan_completed_at timestamptz NOT NULL,
    baseline_generation_snapshot_id uuid NOT NULL
        REFERENCES evaluation_generation_snapshots(id) ON DELETE RESTRICT,
    baseline_generation integer NOT NULL,
    baseline_target_store_path text NOT NULL,
    baseline_occurrence_derivation_path text NOT NULL,
    baseline_observed_package_version text NOT NULL,
    result text NOT NULL CHECK (result IN (
        'pass', 'fail', 'missing', 'whitelisted', 'justified'
    )),
    scan_id uuid,
    scan_derivation_id integer,
    scan_completed_at timestamptz,
    generation_snapshot_id uuid
        REFERENCES evaluation_generation_snapshots(id) ON DELETE RESTRICT,
    generation integer,
    target_store_path text,
    occurrence_present boolean NOT NULL,
    occurrence_derivation_path text,
    observed_package_version text,
    detail text NOT NULL,
    observed_at timestamptz NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (attempt_id, cve_finding_id),
    FOREIGN KEY (
        cve_finding_id,
        system_id,
        canonical_cve_id,
        canonical_package_name
    ) REFERENCES poam_cve_findings(
        id,
        system_id,
        canonical_cve_id,
        canonical_package_name
    ) ON DELETE RESTRICT,
    FOREIGN KEY (
        baseline_scan_id, baseline_scan_derivation_id,
        baseline_scan_completed_at
    ) REFERENCES cve_scans(id, derivation_id, completed_at) ON DELETE RESTRICT,
    FOREIGN KEY (
        baseline_scan_id, baseline_occurrence_derivation_path,
        canonical_cve_id, canonical_package_name
    ) REFERENCES cve_scan_vulnerability_observations(
        scan_id, observed_derivation_path, canonical_cve_id,
        canonical_package_name
    ) ON DELETE RESTRICT,
    FOREIGN KEY (
        scan_id,
        scan_derivation_id,
        scan_completed_at
    ) REFERENCES cve_scans(
        id,
        derivation_id,
        completed_at
    ) ON DELETE RESTRICT,
    FOREIGN KEY (
        scan_id, occurrence_derivation_path, canonical_cve_id,
        canonical_package_name
    ) REFERENCES cve_scan_vulnerability_observations(
        scan_id, observed_derivation_path, canonical_cve_id,
        canonical_package_name
    ) ON DELETE RESTRICT,
    CHECK (
        (result = 'missing' AND scan_id IS NULL
            AND scan_derivation_id IS NULL AND scan_completed_at IS NULL
            AND generation_snapshot_id IS NULL AND generation IS NULL
            AND target_store_path IS NULL)
        OR (result <> 'missing' AND scan_id IS NOT NULL
            AND scan_derivation_id IS NOT NULL AND scan_completed_at IS NOT NULL
            AND generation_snapshot_id IS NOT NULL AND generation IS NOT NULL
            AND btrim(COALESCE(target_store_path, '')) <> '')
    ),
    CHECK (
        (occurrence_present AND occurrence_derivation_path IS NOT NULL
            AND observed_package_version IS NOT NULL)
        OR (NOT occurrence_present AND occurrence_derivation_path IS NULL
            AND observed_package_version IS NULL)
    ),
    CHECK (result <> 'pass' OR NOT occurrence_present),
    CHECK (result NOT IN ('fail', 'whitelisted', 'justified')
        OR occurrence_present),
    CHECK (result <> 'missing' OR NOT occurrence_present)
);

-- PERSISTENCE: PASS proves absence from one sealed latest scan, while positive
-- results bind to an exact immutable occurrence. Existing aggregate policy
-- evaluation remains schema-neutral and does not use this function.
CREATE FUNCTION validate_poam_cve_verification_provenance()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
DECLARE
    v_occurrence_whitelisted boolean;
    v_justified boolean;
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM poam_cve_finding_links link
        JOIN cve_scan_vulnerability_observations baseline
          ON baseline.scan_id=link.baseline_scan_id
         AND baseline.observed_derivation_path=
             link.baseline_occurrence_derivation_path
         AND baseline.canonical_cve_id=link.canonical_cve_id
         AND baseline.canonical_package_name=link.canonical_package_name
        WHERE link.poam_id=(SELECT attempt.poam_id
              FROM poam_verification_attempts attempt
              WHERE attempt.id=NEW.attempt_id)
          AND link.cve_finding_id=NEW.cve_finding_id
          AND link.baseline_scan_id=NEW.baseline_scan_id
          AND link.baseline_scan_derivation_id=NEW.baseline_scan_derivation_id
          AND link.baseline_scan_completed_at=NEW.baseline_scan_completed_at
          AND link.baseline_generation_snapshot_id=
              NEW.baseline_generation_snapshot_id
          AND link.baseline_generation=NEW.baseline_generation
          AND link.baseline_target_store_path=NEW.baseline_target_store_path
          AND link.baseline_occurrence_derivation_path=
              NEW.baseline_occurrence_derivation_path
          AND link.baseline_observed_package_version=
              NEW.baseline_observed_package_version
          AND baseline.observed_package_version=
              NEW.baseline_observed_package_version
    ) THEN
        RAISE EXCEPTION 'CVE verification baseline must match immutable link evidence'
            USING ERRCODE='23514',
                  CONSTRAINT='poam_cve_verification_link_baseline';
    END IF;
    IF NEW.result = 'missing' THEN
        RETURN NEW;
    END IF;
    IF NOT EXISTS (
        SELECT 1
        FROM cve_scans scan
        JOIN systems system ON system.id = NEW.system_id
        JOIN LATERAL (
            SELECT state.store_path,state.generation
            FROM system_states state
            WHERE state.hostname = system.hostname
              AND state.store_path IS NOT NULL
              AND state.generation IS NOT NULL
              AND state.generation_matches_current_store_path IS TRUE
              AND btrim(state.store_path) <> ''
            ORDER BY state.timestamp DESC, state.id DESC
            LIMIT 1
        ) deployed ON deployed.store_path = NEW.target_store_path
        JOIN evaluation_generation_snapshots retained
          ON retained.id=NEW.generation_snapshot_id
         AND retained.system_id=system.id
         AND retained.generation=NEW.generation
         AND retained.generation=deployed.generation
         AND retained.source_store_path=deployed.store_path
         AND retained.derivation_id=scan.derivation_id
         AND retained.lineage_verified
        JOIN evaluation_snapshots artifact
          ON artifact.id=retained.snapshot_id
         AND artifact.commit_id=retained.commit_id
         AND artifact.configuration_name=retained.configuration_name
         AND artifact.lifecycle='available'
         AND artifact.integrity_version=1
        JOIN derivations derivation
          ON derivation.id=retained.derivation_id
         AND derivation.commit_id=retained.commit_id
         AND derivation.derivation_name=retained.configuration_name
         AND derivation.derivation_type='nixos'
         AND COALESCE(derivation.store_path,derivation.expected_store_path)
             =retained.source_store_path
        WHERE scan.id = NEW.scan_id
          AND scan.derivation_id = NEW.scan_derivation_id
          AND scan.completed_at = NEW.scan_completed_at
          AND scan.status = 'completed'
          AND scan.evidence_schema_version = 1
          AND scan.completed_at > NEW.baseline_scan_completed_at
          AND retained.id=NEW.baseline_generation_snapshot_id
          AND retained.generation=NEW.baseline_generation
          AND retained.derivation_id=NEW.baseline_scan_derivation_id
          AND retained.source_store_path=NEW.baseline_target_store_path
          AND COALESCE(derivation.store_path, derivation.expected_store_path)
              = NEW.target_store_path
          AND scan.id = (
              SELECT latest.id
              FROM cve_scans latest
              WHERE latest.derivation_id = scan.derivation_id
                AND latest.status = 'completed'
                AND latest.evidence_schema_version = 1
              ORDER BY latest.completed_at DESC NULLS LAST, latest.id DESC
              LIMIT 1
          )
    ) THEN
        RAISE EXCEPTION 'CVE verification requires a newer sealed scan for the unchanged linked deployment'
            USING ERRCODE = '23514',
                  CONSTRAINT = 'poam_cve_verification_authoritative_scan';
    END IF;

    IF NEW.occurrence_present THEN
        SELECT observation.is_whitelisted INTO v_occurrence_whitelisted
        FROM cve_scan_vulnerability_observations observation
        WHERE observation.scan_id = NEW.scan_id
          AND observation.observed_derivation_path = NEW.occurrence_derivation_path
          AND observation.canonical_cve_id = NEW.canonical_cve_id
          AND observation.canonical_package_name = NEW.canonical_package_name;
        IF NOT FOUND THEN
            RAISE EXCEPTION 'Positive CVE verification requires matching immutable occurrence evidence'
                USING ERRCODE = '23514',
                      CONSTRAINT = 'poam_cve_verification_occurrence_required';
        END IF;
        SELECT EXISTS (
            SELECT 1 FROM system_cve_justifications justification
            WHERE justification.cve_id=NEW.canonical_cve_id
              AND (justification.system_id IS NULL
                OR justification.system_id=NEW.system_id)
        ) INTO v_justified;
    ELSIF NEW.result = 'pass' AND EXISTS (
        SELECT 1 FROM cve_scan_vulnerability_observations observation
        WHERE observation.scan_id = NEW.scan_id
          AND observation.canonical_cve_id = NEW.canonical_cve_id
          AND observation.canonical_package_name = NEW.canonical_package_name
    ) THEN
        RAISE EXCEPTION 'CVE PASS or absence result conflicts with scan occurrence evidence'
            USING ERRCODE = '23514',
                  CONSTRAINT = 'poam_cve_verification_occurrence_absent';
    END IF;
    IF NEW.result = 'fail' AND v_occurrence_whitelisted THEN
        RAISE EXCEPTION 'Unwhitelisted CVE failure cannot cite whitelisted evidence'
            USING ERRCODE = '23514',
                  CONSTRAINT = 'poam_cve_verification_fail_state';
    END IF;
    IF NEW.result = 'fail' AND v_justified THEN
        RAISE EXCEPTION 'Unjustified CVE failure cannot cite justified evidence'
            USING ERRCODE = '23514',
                  CONSTRAINT = 'poam_cve_verification_fail_justification_state';
    END IF;
    IF NEW.result = 'whitelisted' AND NOT v_occurrence_whitelisted THEN
        RAISE EXCEPTION 'Whitelisted CVE result requires whitelisted evidence'
            USING ERRCODE = '23514',
                  CONSTRAINT = 'poam_cve_verification_whitelist_state';
    END IF;
    IF NEW.result = 'justified' AND NOT v_justified THEN
        RAISE EXCEPTION 'Justified CVE result requires an applicable current justification'
            USING ERRCODE = '23514',
                  CONSTRAINT = 'poam_cve_verification_justification_state';
    END IF;
    IF NEW.occurrence_present AND NEW.result <> (CASE
          WHEN v_occurrence_whitelisted THEN 'whitelisted'
          WHEN v_justified THEN 'justified'
          ELSE 'fail'
        END) THEN
        RAISE EXCEPTION 'Positive CVE verification result does not match current evidence state'
            USING ERRCODE = '23514',
                  CONSTRAINT = 'poam_cve_verification_result_state';
    END IF;
    IF NOT NEW.occurrence_present AND NEW.result <> 'pass' THEN
        RAISE EXCEPTION 'Absent CVE occurrence requires a PASS result'
            USING ERRCODE = '23514',
                  CONSTRAINT = 'poam_cve_verification_absence_result';
    END IF;
    IF NEW.occurrence_present AND NEW.observed_package_version IS DISTINCT FROM (
        SELECT observation.observed_package_version
        FROM cve_scan_vulnerability_observations observation
        WHERE observation.scan_id=NEW.scan_id
          AND observation.observed_derivation_path=NEW.occurrence_derivation_path
          AND observation.canonical_cve_id=NEW.canonical_cve_id
          AND observation.canonical_package_name=NEW.canonical_package_name
    ) THEN
        RAISE EXCEPTION 'CVE verification package version must match occurrence evidence'
            USING ERRCODE = '23514',
                  CONSTRAINT = 'poam_cve_verification_package_version';
    END IF;
    RETURN NEW;
END;
$$;

ALTER TABLE poam_activity DROP CONSTRAINT poam_activity_kind_check;
ALTER TABLE poam_activity ADD CONSTRAINT poam_activity_kind_check CHECK (kind IN (
    'created', 'updated', 'status_changed', 'milestone_added',
    'milestone_updated', 'milestone_removed', 'note', 'finding_linked',
    'finding_unlinked', 'cve_finding_linked', 'cve_finding_unlinked',
    'assignment_linked', 'assignment_unlinked', 'verification_attempted',
    'closed', 'reopened'
));

CREATE TRIGGER trigger_require_unsealed_cve_verification_attempt
    BEFORE INSERT ON poam_cve_verification_items
    FOR EACH ROW EXECUTE FUNCTION require_unsealed_verification_attempt();
CREATE TRIGGER trigger_validate_poam_cve_verification_provenance
    BEFORE INSERT ON poam_cve_verification_items
    FOR EACH ROW EXECUTE FUNCTION validate_poam_cve_verification_provenance();
CREATE TRIGGER trigger_prevent_poam_cve_verification_item_mutation
    BEFORE UPDATE OR DELETE ON poam_cve_verification_items
    FOR EACH ROW EXECUTE FUNCTION prevent_poam_history_mutation();

-- CONCURRENCY: Both finding families lock the same parent before changing a
-- link. Concurrent policy and CVE inserts therefore serialize before either
-- row becomes visible, and the deferred cross-table check cannot let both
-- transactions commit based on disjoint snapshots.
CREATE FUNCTION lock_poam_finding_family_parent()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
DECLARE
    v_poam_id uuid;
BEGIN
    v_poam_id := CASE WHEN TG_OP = 'DELETE' THEN OLD.poam_id ELSE NEW.poam_id END;
    PERFORM 1 FROM poams WHERE id = v_poam_id FOR UPDATE;
    IF TG_OP = 'DELETE' THEN
        RETURN OLD;
    END IF;
    RETURN NEW;
END;
$$;
CREATE TRIGGER trigger_lock_policy_finding_family_parent
    BEFORE INSERT OR UPDATE OR DELETE ON poam_finding_links
    FOR EACH ROW EXECUTE FUNCTION lock_poam_finding_family_parent();
CREATE TRIGGER trigger_lock_cve_finding_family_parent
    BEFORE INSERT OR UPDATE OR DELETE ON poam_cve_finding_links
    FOR EACH ROW EXECUTE FUNCTION lock_poam_finding_family_parent();

-- INVARIANT: A live POA&M has exactly one active finding family. Policy-only
-- behavior is unchanged; the second family is additive and mutually exclusive.
CREATE OR REPLACE FUNCTION require_active_poam_finding()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
DECLARE
    v_poam_id uuid;
    v_has_policy boolean;
    v_has_cve boolean;
    v_status text;
BEGIN
    IF TG_TABLE_NAME = 'poams' THEN
        v_poam_id := COALESCE(
            (to_jsonb(NEW)->>'id')::uuid,
            (to_jsonb(OLD)->>'id')::uuid
        );
    ELSE
        v_poam_id := COALESCE(
            (to_jsonb(NEW)->>'poam_id')::uuid,
            (to_jsonb(OLD)->>'poam_id')::uuid
        );
    END IF;
    SELECT status INTO v_status FROM poams WHERE id = v_poam_id FOR UPDATE;
    IF NOT FOUND THEN
        RETURN COALESCE(NEW, OLD);
    END IF;
    SELECT EXISTS (
        SELECT 1 FROM poam_finding_links
        WHERE poam_id = v_poam_id
    ) INTO v_has_policy;
    SELECT EXISTS (
        SELECT 1 FROM poam_cve_finding_links
        WHERE poam_id = v_poam_id
    ) INTO v_has_cve;

    IF v_has_policy AND v_has_cve THEN
        RAISE EXCEPTION 'A POA&M cannot mix policy and CVE finding history'
            USING ERRCODE = '23514',
                  CONSTRAINT = 'poams_one_active_finding_type';
    END IF;
    IF v_has_cve AND EXISTS (
        SELECT 1 FROM poam_cve_finding_links first_link
        JOIN poam_cve_finding_links other ON other.poam_id=first_link.poam_id
        WHERE first_link.poam_id=v_poam_id
          AND (other.canonical_cve_id<>first_link.canonical_cve_id
            OR other.canonical_package_name<>first_link.canonical_package_name)
    ) THEN
        RAISE EXCEPTION 'CVE findings in one POA&M must share canonical identity'
            USING ERRCODE = '23514',
                  CONSTRAINT = 'poams_one_cve_identity';
    END IF;
    SELECT EXISTS (SELECT 1 FROM poam_finding_links
        WHERE poam_id=v_poam_id AND retired_at IS NULL) INTO v_has_policy;
    SELECT EXISTS (SELECT 1 FROM poam_cve_finding_links
        WHERE poam_id=v_poam_id AND retired_at IS NULL) INTO v_has_cve;
    IF v_status <> 'completed' AND NOT (v_has_policy OR v_has_cve) THEN
        RAISE EXCEPTION 'A non-completed POA&M requires an active finding'
            USING ERRCODE = '23514',
                  CONSTRAINT = 'poams_active_finding_required';
    END IF;
    IF v_status = 'completed' AND (v_has_policy OR v_has_cve) THEN
        RAISE EXCEPTION 'A completed POA&M cannot have an active finding'
            USING ERRCODE = '23514',
                  CONSTRAINT = 'poams_completed_without_active_finding';
    END IF;
    IF TG_OP = 'DELETE' THEN
        RETURN OLD;
    END IF;
    RETURN NEW;
END;
$$;

CREATE CONSTRAINT TRIGGER trigger_poam_cve_links_require_active_finding
    AFTER INSERT OR UPDATE OR DELETE ON poam_cve_finding_links
    DEFERRABLE INITIALLY DEFERRED
    FOR EACH ROW EXECUTE FUNCTION require_active_poam_finding();

CREATE VIEW poam_current_cve_finding_links AS
SELECT link.*
FROM poam_cve_finding_links link
JOIN poams poam ON poam.id = link.poam_id
WHERE (poam.status <> 'completed' AND link.retired_at IS NULL)
   OR (poam.status = 'completed'
       AND link.retirement_reason='closed:'||poam.closure_attempt_id::text);

CREATE OR REPLACE VIEW poam_context_systems AS
SELECT link.poam_id, finding.system_id
FROM poam_current_finding_links link
JOIN poam_findings finding ON finding.id = link.finding_id
UNION
SELECT link.poam_id, link.system_id
FROM poam_current_cve_finding_links link
UNION
SELECT reference.poam_id, system.id
FROM poam_assignment_references reference
JOIN compliance_bundle_assignment_versions version
  ON version.id = reference.assignment_version_id
JOIN compliance_bundle_assignments assignment ON assignment.id = version.assignment_id
JOIN systems system ON system.id = assignment.system_id
   OR system.environment_id = assignment.environment_id;

CREATE OR REPLACE FUNCTION poam_visible_to_environments(
    v_poam_id uuid,
    v_environment_ids uuid[]
) RETURNS boolean LANGUAGE sql STABLE AS $$
SELECT (EXISTS (SELECT 1 FROM poam_current_finding_links WHERE poam_id = v_poam_id)
        OR EXISTS (SELECT 1 FROM poam_current_cve_finding_links WHERE poam_id = v_poam_id)
        OR EXISTS (SELECT 1 FROM poam_assignment_references WHERE poam_id = v_poam_id))
  AND NOT EXISTS (
    SELECT 1 FROM poam_context_systems context
    JOIN systems system ON system.id = context.system_id
    WHERE context.poam_id = v_poam_id
      AND (system.environment_id IS NULL
        OR NOT (system.environment_id = ANY(v_environment_ids))))
  AND NOT EXISTS (
    SELECT 1 FROM poam_assignment_references reference
    JOIN compliance_bundle_assignment_versions version
      ON version.id = reference.assignment_version_id
    JOIN compliance_bundle_assignments assignment ON assignment.id = version.assignment_id
    LEFT JOIN systems assigned_system ON assigned_system.id = assignment.system_id
    WHERE reference.poam_id = v_poam_id
      AND (COALESCE(assignment.environment_id, assigned_system.environment_id) IS NULL
        OR NOT (COALESCE(assignment.environment_id, assigned_system.environment_id)
            = ANY(v_environment_ids))));
$$;

-- CONCURRENCY: Deployment publication follows the canonical CVE, system,
-- policy, then exact-CVE key order. UPDATE and DELETE use nonblocking advisory
-- locks because PostgreSQL has already locked the mutable row before this
-- trigger runs.
CREATE OR REPLACE FUNCTION lock_poam_findings_for_system_state()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
DECLARE
    v_system record;
    v_key record;
BEGIN
    FOR v_key IN SELECT DISTINCT canonical_cve_id FROM poam_cve_findings
        WHERE system_id IN (SELECT system.id FROM systems system
          WHERE system.hostname IN (
            CASE WHEN TG_OP='INSERT' THEN NEW.hostname ELSE OLD.hostname END,
            CASE WHEN TG_OP='DELETE' THEN OLD.hostname ELSE NEW.hostname END))
        ORDER BY canonical_cve_id
    LOOP
        IF TG_OP='INSERT' THEN
            PERFORM lock_poam_cve_key(v_key.canonical_cve_id);
        ELSE
            PERFORM try_lock_poam_cve_key(v_key.canonical_cve_id);
        END IF;
    END LOOP;
    FOR v_system IN
        SELECT system.id FROM systems system
        WHERE system.hostname IN (
            CASE WHEN TG_OP='INSERT' THEN NEW.hostname ELSE OLD.hostname END,
            CASE WHEN TG_OP='DELETE' THEN OLD.hostname ELSE NEW.hostname END)
        ORDER BY system.id
    LOOP
        IF TG_OP='INSERT' THEN
            PERFORM lock_poam_finding_key(
                v_system.id,'00000000-0000-0000-0000-000000000000');
        ELSE
            PERFORM try_lock_poam_finding_key(
                v_system.id,'00000000-0000-0000-0000-000000000000');
        END IF;
        FOR v_key IN SELECT system_id,policy_lineage_id FROM poam_findings
            WHERE system_id=v_system.id ORDER BY system_id,policy_lineage_id
        LOOP
            IF TG_OP='INSERT' THEN
                PERFORM lock_poam_finding_key(v_key.system_id,v_key.policy_lineage_id);
            ELSE
                PERFORM try_lock_poam_finding_key(v_key.system_id,v_key.policy_lineage_id);
            END IF;
        END LOOP;
        FOR v_key IN SELECT system_id,canonical_cve_id,canonical_package_name
            FROM poam_cve_findings WHERE system_id=v_system.id
            ORDER BY system_id,canonical_cve_id,canonical_package_name
        LOOP
            IF TG_OP='INSERT' THEN
                PERFORM lock_poam_cve_finding_key(v_key.system_id,
                    v_key.canonical_cve_id,v_key.canonical_package_name);
            ELSE
                PERFORM try_lock_poam_cve_finding_key(v_key.system_id,
                    v_key.canonical_cve_id,v_key.canonical_package_name);
            END IF;
        END LOOP;
    END LOOP;
    IF TG_OP='DELETE' THEN RETURN OLD; END IF;
    RETURN NEW;
END;
$$;

CREATE OR REPLACE FUNCTION lock_poam_findings_for_system_metadata()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
DECLARE
    v_system_id uuid;
    v_key record;
BEGIN
    -- CONCURRENCY: The HTTP metadata writer pre-acquires this complete lock set.
    -- Other writers must retry instead of waiting after the systems row lock.
    FOR v_key IN SELECT DISTINCT canonical_cve_id FROM poam_cve_findings
        WHERE system_id IN (OLD.id,NEW.id) ORDER BY canonical_cve_id
    LOOP
        PERFORM try_lock_poam_cve_key(v_key.canonical_cve_id);
    END LOOP;
    FOR v_system_id IN SELECT id FROM unnest(ARRAY[OLD.id,NEW.id]) id ORDER BY id
    LOOP
        PERFORM try_lock_poam_finding_key(
            v_system_id,'00000000-0000-0000-0000-000000000000');
        FOR v_key IN SELECT system_id,policy_lineage_id FROM poam_findings
            WHERE system_id=v_system_id ORDER BY system_id,policy_lineage_id
        LOOP
            PERFORM try_lock_poam_finding_key(v_key.system_id,v_key.policy_lineage_id);
        END LOOP;
        FOR v_key IN SELECT system_id,canonical_cve_id,canonical_package_name
            FROM poam_cve_findings WHERE system_id=v_system_id
            ORDER BY system_id,canonical_cve_id,canonical_package_name
        LOOP
            PERFORM try_lock_poam_cve_finding_key(v_key.system_id,
                v_key.canonical_cve_id,v_key.canonical_package_name);
        END LOOP;
    END LOOP;
    RETURN NEW;
END;
$$;

-- SECURITY: This function preserves every policy closure check from migration
-- 0242 and adds a disjoint exact-CVE branch. For CVEs, only current absence in
-- the latest sealed scan for the unchanged exact deployment accepts closure.
CREATE OR REPLACE FUNCTION validate_poam_closure_evidence()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
DECLARE
    v_has_policy boolean;
    v_has_cve boolean;
BEGIN
  IF NEW.status<>'completed' THEN RETURN NEW; END IF;
  SELECT EXISTS(SELECT 1 FROM poam_finding_links WHERE poam_id=NEW.id)
    INTO v_has_policy;
  SELECT EXISTS(SELECT 1 FROM poam_cve_finding_links WHERE poam_id=NEW.id)
    INTO v_has_cve;
  IF v_has_policy=v_has_cve OR NOT EXISTS (
      SELECT 1 FROM poam_verification_attempts attempt
      WHERE attempt.id=NEW.closure_attempt_id AND attempt.poam_id=NEW.id
        AND attempt.outcome='accepted' AND attempt.sealed_at IS NOT NULL
  ) THEN
    RAISE EXCEPTION 'Completed POA&M closure evidence is incomplete or inconsistent'
      USING ERRCODE='23514',CONSTRAINT='poams_authoritative_closure_evidence';
  END IF;

  IF v_has_policy AND (
    EXISTS (SELECT 1 FROM poam_cve_verification_items
            WHERE attempt_id=NEW.closure_attempt_id)
    OR NOT EXISTS (SELECT 1 FROM poam_verification_items
                   WHERE attempt_id=NEW.closure_attempt_id)
    OR EXISTS (
      (SELECT link.finding_id FROM poam_finding_links link
       WHERE link.poam_id=NEW.id
         AND link.retirement_reason='closed:'||NEW.closure_attempt_id::text)
      EXCEPT
      (SELECT item.finding_id FROM poam_verification_items item
       WHERE item.attempt_id=NEW.closure_attempt_id)
    )
    OR EXISTS (
      (SELECT item.finding_id FROM poam_verification_items item
       WHERE item.attempt_id=NEW.closure_attempt_id)
      EXCEPT
      (SELECT link.finding_id FROM poam_finding_links link
       WHERE link.poam_id=NEW.id
         AND link.retirement_reason='closed:'||NEW.closure_attempt_id::text)
    )
    OR EXISTS (
      SELECT 1 FROM poam_verification_items item
      WHERE item.attempt_id=NEW.closure_attempt_id
        AND NOT (
          (item.result='pass' AND item.observed_outcome='pass'
           AND item.waiver_id IS NULL AND (
             (item.assessment_id IS NOT NULL
              AND item.effective_context_attestation_id IS NULL
              AND EXISTS (
                SELECT 1 FROM composite_policy_assessments assessment
                WHERE assessment.id=item.assessment_id
                  AND assessment.system_id=item.system_id
                  AND assessment.policy_lineage_id=item.policy_lineage_id
                  AND assessment.policy_version_id=item.policy_version_id
                  AND assessment.derivation_id=item.derivation_id
                  AND assessment.target_store_path=item.target_store_path
                  AND assessment.effective_set_digest=item.effective_set_digest
                  AND assessment.effective_config_digest=item.effective_config_digest
                  AND assessment.effective_config=item.effective_config
                  AND assessment.updated_at=item.assessment_updated_at
                  AND assessment.overall_outcome='pass'
                  AND item.observation_token=encode(digest(
                    canonical_poam_observation_json(item.observation_snapshot),'sha256'),'hex')
                  AND item.observation_snapshot=jsonb_build_object(
                    'assessment',to_jsonb(assessment),
                    'rules',COALESCE((SELECT jsonb_agg(to_jsonb(rule_result)
                      ORDER BY rule_result.ordinal,rule_result.rule_id)
                      FROM composite_policy_rule_results rule_result
                      WHERE rule_result.assessment_id=assessment.id),'[]'::jsonb))
              ))
             OR (item.assessment_id IS NULL
              AND item.effective_context_attestation_id IS NOT NULL
              AND poam_effective_attestation_matches(item)
              AND poam_legacy_observation_is_authoritative(item,TRUE))
           ))
          OR (item.result='waiver' AND item.observed_outcome='fail'
           AND EXISTS (
             SELECT 1 FROM finding_waivers waiver
             WHERE waiver.id=item.waiver_id AND waiver.finding_id=item.finding_id
               AND waiver.assessment_id IS NOT DISTINCT FROM item.assessment_id
               AND waiver.policy_version_id=item.policy_version_id
               AND waiver.observation_token=item.observation_token
               AND waiver.observation_snapshot=item.observation_snapshot
               AND waiver.status='accepted' AND waiver.accepted_at<=CURRENT_TIMESTAMP
               AND (waiver.expires_at IS NULL OR waiver.expires_at>CURRENT_TIMESTAMP)
           ) AND (
             (item.assessment_id IS NOT NULL
              AND item.effective_context_attestation_id IS NULL
              AND item.observation_snapshot->'assessment'->>'overall_outcome'='fail')
             OR (item.assessment_id IS NULL
              AND item.effective_context_attestation_id IS NOT NULL
              AND poam_effective_attestation_matches(item)
              AND poam_legacy_observation_is_authoritative(item,FALSE))
           ))
        )
    )
  ) THEN
    RAISE EXCEPTION 'Completed POA&M closure evidence is incomplete or inconsistent'
      USING ERRCODE='23514',CONSTRAINT='poams_authoritative_closure_evidence';
  END IF;

  IF v_has_cve AND (
    EXISTS (SELECT 1 FROM poam_verification_items
            WHERE attempt_id=NEW.closure_attempt_id)
    OR NOT EXISTS (SELECT 1 FROM poam_cve_verification_items
                   WHERE attempt_id=NEW.closure_attempt_id)
    OR EXISTS (
      (SELECT link.cve_finding_id FROM poam_cve_finding_links link
       WHERE link.poam_id=NEW.id
         AND link.retirement_reason='closed:'||NEW.closure_attempt_id::text)
      EXCEPT
      (SELECT item.cve_finding_id FROM poam_cve_verification_items item
       WHERE item.attempt_id=NEW.closure_attempt_id)
    )
    OR EXISTS (
      (SELECT item.cve_finding_id FROM poam_cve_verification_items item
       WHERE item.attempt_id=NEW.closure_attempt_id)
      EXCEPT
      (SELECT link.cve_finding_id FROM poam_cve_finding_links link
       WHERE link.poam_id=NEW.id
         AND link.retirement_reason='closed:'||NEW.closure_attempt_id::text)
    )
    OR EXISTS (
      SELECT 1 FROM poam_cve_verification_items item
      WHERE item.attempt_id=NEW.closure_attempt_id
        AND (item.result<>'pass' OR item.occurrence_present
          OR NOT EXISTS (
            SELECT 1 FROM poam_cve_finding_links baseline_link
            WHERE baseline_link.poam_id=NEW.id
              AND baseline_link.cve_finding_id=item.cve_finding_id
              AND baseline_link.retirement_reason=
                  'closed:'||NEW.closure_attempt_id::text
              AND baseline_link.baseline_scan_id=item.baseline_scan_id
              AND baseline_link.baseline_scan_derivation_id=
                  item.baseline_scan_derivation_id
              AND baseline_link.baseline_scan_completed_at=
                  item.baseline_scan_completed_at
              AND baseline_link.baseline_generation_snapshot_id=
                  item.baseline_generation_snapshot_id
              AND baseline_link.baseline_generation=item.baseline_generation
              AND baseline_link.baseline_target_store_path=
                  item.baseline_target_store_path
              AND baseline_link.baseline_occurrence_derivation_path=
                  item.baseline_occurrence_derivation_path
              AND baseline_link.baseline_observed_package_version=
                  item.baseline_observed_package_version)
          OR NOT EXISTS (
            SELECT 1 FROM systems system
            JOIN LATERAL (
              SELECT state.store_path,state.generation FROM system_states state
              WHERE state.hostname=system.hostname AND state.store_path IS NOT NULL
                AND state.generation IS NOT NULL
                AND state.generation_matches_current_store_path IS TRUE
                AND btrim(state.store_path)<>''
              ORDER BY state.timestamp DESC,state.id DESC LIMIT 1
            ) deployed ON deployed.store_path=item.target_store_path
            JOIN evaluation_generation_snapshots retained
              ON retained.id=item.generation_snapshot_id
             AND retained.system_id=system.id
             AND retained.generation=item.generation
             AND retained.generation=deployed.generation
             AND retained.source_store_path=deployed.store_path
             AND retained.derivation_id=item.scan_derivation_id
             AND retained.lineage_verified
            JOIN evaluation_snapshots artifact
              ON artifact.id=retained.snapshot_id
             AND artifact.commit_id=retained.commit_id
             AND artifact.configuration_name=retained.configuration_name
             AND artifact.lifecycle='available'
             AND artifact.integrity_version=1
            JOIN derivations derivation
              ON derivation.id=retained.derivation_id
             AND derivation.commit_id=retained.commit_id
             AND derivation.derivation_name=retained.configuration_name
             AND derivation.derivation_type='nixos'
             AND COALESCE(derivation.store_path,derivation.expected_store_path)=retained.source_store_path
            JOIN cve_scans scan
              ON scan.id=item.scan_id AND scan.derivation_id=derivation.id
              AND scan.completed_at=item.scan_completed_at
              AND scan.status='completed' AND scan.evidence_schema_version=1
              AND scan.completed_at>item.baseline_scan_completed_at
              AND retained.id=item.baseline_generation_snapshot_id
              AND retained.generation=item.baseline_generation
              AND retained.derivation_id=item.baseline_scan_derivation_id
              AND retained.source_store_path=item.baseline_target_store_path
            WHERE system.id=item.system_id
              AND scan.id=(SELECT latest.id FROM cve_scans latest
                WHERE latest.derivation_id=derivation.id
                  AND latest.status='completed' AND latest.evidence_schema_version=1
                ORDER BY latest.completed_at DESC,latest.id DESC LIMIT 1)
              AND NOT EXISTS (
                SELECT 1 FROM cve_scan_vulnerability_observations observation
                WHERE observation.scan_id=scan.id
                  AND observation.canonical_cve_id=item.canonical_cve_id
                  AND observation.canonical_package_name=item.canonical_package_name)
          ))
    )
  ) THEN
    RAISE EXCEPTION 'Completed POA&M closure evidence is incomplete or inconsistent'
      USING ERRCODE='23514',CONSTRAINT='poams_authoritative_closure_evidence';
  END IF;
  RETURN NEW;
END;
$$;

-- TASK-508 AC8: evaluation identity is distinct from physical Nix identity.
-- Existing source IDs and execution evidence are never rewritten or backfilled.
ALTER TABLE derivations
    ADD COLUMN reused_from_derivation_id integer
        REFERENCES derivations(id) ON DELETE RESTRICT,
    ADD COLUMN reused_build_job_id uuid REFERENCES build_jobs(id) ON DELETE RESTRICT,
    ADD CONSTRAINT derivations_reuse_not_self CHECK (reused_from_derivation_id <> id);

CREATE INDEX derivations_reuse_origin_idx ON derivations(reused_from_derivation_id)
    WHERE reused_from_derivation_id IS NOT NULL;

-- A physical identity is not evidence visibility, build admission, successful
-- execution, publication, or runtime Git provenance. Callers retain those gates.
CREATE FUNCTION derivation_physical_id(target_id integer) RETURNS integer
LANGUAGE sql STABLE STRICT AS $$
    SELECT COALESCE(reused_from_derivation_id,id) FROM derivations WHERE id=target_id
$$;

-- Failure publication invalidates evaluation permission, not an already bound
-- physical carrier. Unbound pending evaluation fields can still be normalized.
CREATE FUNCTION derivation_identity_is_bound(target_id integer) RETURNS boolean
LANGUAGE sql STABLE STRICT AS $$
    SELECT EXISTS (
        SELECT 1 FROM derivations d WHERE d.id=target_id AND (
            d.reused_from_derivation_id IS NOT NULL OR d.reused_build_job_id IS NOT NULL
            OR EXISTS (SELECT 1 FROM derivations alias WHERE alias.reused_from_derivation_id=d.id)
            OR (d.derivation_path IS NOT NULL AND EXISTS (SELECT 1 FROM build_jobs job WHERE job.derivation_id=d.id))
            OR EXISTS (SELECT 1 FROM cve_scans scan WHERE scan.derivation_id=d.id
                       AND (scan.evidence_schema_version=1 OR d.derivation_path IS NOT NULL))
            OR EXISTS (SELECT 1 FROM evaluation_generation_snapshots retained WHERE retained.derivation_id=d.id)
            OR EXISTS (SELECT 1 FROM pending_system_deployments pending
                       WHERE pending.requested_derivation_id=d.id AND pending.evaluation_snapshot_id IS NOT NULL)
            OR EXISTS (SELECT 1 FROM config_inspection_jobs inspection
                       WHERE inspection.derivation_id=d.id AND inspection.carrier_drv_path IS NOT NULL)
            OR EXISTS (SELECT 1 FROM composite_policy_assessments assessment
                       JOIN composite_policy_rule_results rule ON rule.assessment_id=assessment.id
                       WHERE assessment.derivation_id=d.id AND rule.source_scan_id IS NOT NULL)
        )
    )
$$;

CREATE FUNCTION guard_exact_derivation_reuse() RETURNS trigger
LANGUAGE plpgsql AS $$
DECLARE
    origin derivations%ROWTYPE;
BEGIN
    -- UPDATE already owns its row lock. Alias creation takes SHARE on the
    -- origin below, which conflicts with identity updates through commit.
    -- Volatile trigger queries inspect aliases after a concurrent writer ends.
    IF TG_OP = 'UPDATE' THEN
        IF ROW(NEW.derivation_path,NEW.expected_store_path,NEW.derivation_type,
               NEW.reused_from_derivation_id,NEW.commit_id,NEW.derivation_name)
           IS DISTINCT FROM
           ROW(OLD.derivation_path,OLD.expected_store_path,OLD.derivation_type,
               OLD.reused_from_derivation_id,OLD.commit_id,OLD.derivation_name)
           AND derivation_identity_is_bound(OLD.id)
        THEN
            RAISE EXCEPTION 'Referenced physical derivation identity is immutable';
        END IF;
        IF EXISTS (SELECT 1 FROM derivations WHERE reused_from_derivation_id=OLD.id)
           AND (COALESCE(NEW.expected_store_path,NEW.store_path)
                    IS DISTINCT FROM COALESCE(OLD.expected_store_path,OLD.store_path)
                OR (NEW.store_path IS NOT NULL AND NEW.expected_store_path IS NOT NULL
                    AND NEW.store_path IS DISTINCT FROM NEW.expected_store_path)
                OR lower(COALESCE(NEW.policy_results->>'execution_mode','native'))='mock') THEN
            RAISE EXCEPTION 'Referenced native origin cannot change output or become simulated';
        END IF;
    END IF;
    IF NEW.reused_from_derivation_id IS NULL THEN RETURN NEW; END IF;
    SELECT * INTO origin FROM derivations WHERE id=NEW.reused_from_derivation_id FOR SHARE;
    IF NOT FOUND OR origin.reused_from_derivation_id IS NOT NULL
       OR origin.id=NEW.id OR origin.derivation_type<>'nixos'
       OR NEW.derivation_type<>'nixos'
       OR NEW.derivation_path IS NULL
       OR NEW.derivation_path !~ '^/nix/store/[^/[:space:]]+\.drv$'
       OR NEW.derivation_path IS DISTINCT FROM origin.derivation_path
       OR NEW.expected_store_path IS NULL
       OR NEW.expected_store_path !~ '^/nix/store/[^/[:space:]]+$'
       OR NEW.expected_store_path IS DISTINCT FROM COALESCE(origin.expected_store_path,origin.store_path)
       OR (origin.store_path IS NOT NULL AND origin.store_path<>NEW.expected_store_path)
       OR (NEW.store_path IS NOT NULL AND NEW.store_path<>NEW.expected_store_path)
       OR lower(COALESCE(NEW.policy_results->>'execution_mode','native'))='mock'
       OR lower(COALESCE(origin.policy_results->>'execution_mode','native'))='mock'
    THEN
        RAISE EXCEPTION 'Derivation reuse requires a flat exact native drv/output binding';
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER exact_derivation_reuse_guard BEFORE INSERT OR UPDATE ON derivations
    FOR EACH ROW EXECUTE FUNCTION guard_exact_derivation_reuse();

COMMENT ON COLUMN derivations.reused_from_derivation_id IS
    'Flat physical Nix identity only. The referenced first evaluation need not own a build or scan. Every real execution retains its own derivation, commit, policy, environment and session. No cross-scope evidence permission is implied.';

-- Physical equality does not cross configuration/source visibility boundaries.
-- This predicate is for evidence joins AFTER the requesting subject is scoped.
CREATE FUNCTION derivations_share_evidence_scope(target_id integer, source_id integer)
RETURNS boolean LANGUAGE sql STABLE STRICT AS $$
    SELECT EXISTS (
        SELECT 1 FROM derivations target JOIN commits tc ON tc.id=target.commit_id
        JOIN derivations source ON source.id=source_id
        JOIN commits sc ON sc.id=source.commit_id
        WHERE target.id=target_id AND tc.flake_id=sc.flake_id
          AND target.derivation_type='nixos' AND source.derivation_type='nixos'
          AND target.derivation_name=source.derivation_name
          AND target.derivation_path IS NOT NULL
          AND target.derivation_path=source.derivation_path
          AND COALESCE(target.expected_store_path,target.store_path) IS NOT NULL
          AND COALESCE(target.expected_store_path,target.store_path)
              =COALESCE(source.expected_store_path,source.store_path)
          AND COALESCE(target.reused_from_derivation_id,target.id)
              =COALESCE(source.reused_from_derivation_id,source.id)
    )
$$;

-- Admission-only publication proof. This does not return credentials or bypass
-- delivery/scanner transport, capability, signature, or current-cache checks.
-- Legacy identity must resolve uniquely BEFORE enabled/environment filtering.
CREATE FUNCTION derivation_publication_eligible(source_id integer, output_path text, subject_environment uuid)
RETURNS boolean LANGUAGE sql STABLE AS $$
    SELECT EXISTS (
        SELECT 1 FROM cache_push_jobs push JOIN cache_destinations destination ON
          ((push.cache_destination_source='database' AND destination.id=push.cache_destination_id)
           OR (push.cache_destination_source='legacy' AND push.cache_destination_id IS NULL
               AND (destination.name=push.cache_destination OR destination.push_to=push.cache_destination)
               AND (SELECT count(*) FROM cache_destinations candidate
                    WHERE candidate.name=push.cache_destination OR candidate.push_to=push.cache_destination)=1))
        WHERE push.derivation_id=source_id AND push.store_path=output_path
          AND push.status='completed' AND push.completed_at IS NOT NULL AND destination.enabled
          AND (NOT EXISTS (SELECT 1 FROM cache_destination_environments assignment
                           WHERE assignment.cache_destination_id=destination.id)
               OR EXISTS (SELECT 1 FROM cache_destination_environments assignment
                          WHERE assignment.cache_destination_id=destination.id
                            AND assignment.environment_id=subject_environment))
    )
$$;

-- Execution provenance can belong to an alias, not the first evaluator. Only a
-- real successful execution supplies inherited output. No expected path is
-- promoted to a built output. Legacy own-row evidence remains unchanged.
CREATE FUNCTION derivation_execution_source(target_id integer) RETURNS integer
LANGUAGE sql STABLE STRICT AS $$
    SELECT COALESCE((
        SELECT source.id FROM derivations target
        JOIN build_jobs job ON job.id=target.reused_build_job_id
        JOIN derivations source ON source.id=job.derivation_id
        WHERE target.id=target_id AND derivations_share_evidence_scope(target.id,source.id)
          AND (job.status IN ('queued','building','cancelling')
               OR (job.status='success' AND source.store_path=target.expected_store_path))
    ),target_id)
$$;

-- Pin the actual adopted execution, which need not belong to the first physical
-- evaluator. Restrictive references retain its immutable job/scan provenance.
CREATE FUNCTION guard_reused_build_job() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF NEW.reused_build_job_id IS NULL THEN RETURN NEW; END IF;
    IF NOT EXISTS (
        SELECT 1 FROM build_jobs job JOIN derivations source ON source.id=job.derivation_id
        JOIN commits sc ON sc.id=source.commit_id
        JOIN commits tc ON tc.id=NEW.commit_id AND tc.flake_id=sc.flake_id
        WHERE job.id=NEW.reused_build_job_id AND source.id<>NEW.id
          AND source.derivation_type='nixos' AND NEW.derivation_type='nixos'
          AND source.derivation_name=NEW.derivation_name
          AND source.derivation_path=NEW.derivation_path
          AND COALESCE(source.expected_store_path,source.store_path)=NEW.expected_store_path
          AND COALESCE(source.reused_from_derivation_id,source.id)
              =COALESCE(NEW.reused_from_derivation_id,NEW.id)
    ) THEN RAISE EXCEPTION 'Adopted job requires exact physical and visible source identity'; END IF;
    IF TG_OP='INSERT' OR NEW.reused_build_job_id IS DISTINCT FROM OLD.reused_build_job_id THEN
        IF NOT EXISTS (SELECT 1 FROM build_jobs job WHERE job.id=NEW.reused_build_job_id
                       AND job.status IN ('queued','building','cancelling','success')) THEN
            RAISE EXCEPTION 'Only active or successful real work can be adopted';
        END IF;
    END IF;
    RETURN NEW;
END;
$$;
CREATE TRIGGER reused_build_job_guard BEFORE INSERT OR UPDATE ON derivations
    FOR EACH ROW EXECUTE FUNCTION guard_reused_build_job();

COMMENT ON COLUMN derivations.reused_build_job_id IS
    'Real adopted execution, distinct from the physical origin. Pins actual job provenance without creating an execution on this configuration. A later failure may authorize normal preparation under this configuration policies; it never transfers the old builder session.';

-- Keep the actual row as the configuration carrier. Consumers must use the
-- explicit execution ID for jobs/publications, never attach those events to B.
CREATE VIEW view_derivation_execution_evidence AS
SELECT target.id,target.commit_id,target.derivation_type,target.derivation_name,
       target.derivation_path,target.derivation_target,target.expected_store_path,
       target.cf_agent_enabled,target.policy_requirements_met,target.policy_results,
       target.error_message,target.status_id,target.completed_at,
       target.reused_from_derivation_id,
       target.reused_build_job_id,
       COALESCE(target.reused_from_derivation_id,target.id) AS physical_derivation_id,
       source.id AS execution_derivation_id,source.commit_id AS execution_commit_id,
       source.store_path,
       -- EvaluationStatus IDs describe the selected real job here, not B's
       -- evaluation or the source row's newer failed retry. No status is copied.
       CASE adopted.status WHEN 'success' THEN 10 WHEN 'queued' THEN 7
            WHEN 'building' THEN 8 WHEN 'cancelling' THEN 8
            ELSE source.status_id END AS execution_status_id
FROM derivations target JOIN derivations source ON source.id=derivation_execution_source(target.id)
LEFT JOIN build_jobs adopted ON adopted.id=target.reused_build_job_id AND adopted.derivation_id=source.id;

-- The existing wider (id,path) and (id,commit,name,path) keys remain intact.
-- Only proven aliases can repeat a path. Package upserts target origins only.
ALTER TABLE derivations DROP CONSTRAINT derivations_derivation_path_unique;
CREATE UNIQUE INDEX derivations_origin_path_unique ON derivations(derivation_path)
    WHERE reused_from_derivation_id IS NULL;

-- Supplemental physical deduplication never changes a job's execution source.
-- Existing jobs precede aliases, so their own derivation is their physical key.
ALTER TABLE build_jobs ADD COLUMN physical_derivation_id integer;

CREATE FUNCTION bind_build_physical_identity() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP='UPDATE' AND NEW.derivation_id IS DISTINCT FROM OLD.derivation_id THEN
        RAISE EXCEPTION 'Build execution source is immutable';
    END IF;
    NEW.physical_derivation_id := derivation_physical_id(NEW.derivation_id);
    IF NEW.physical_derivation_id IS NULL THEN
        RAISE EXCEPTION 'Build requires an existing physical derivation';
    END IF;
    RETURN NEW;
END;
$$;
CREATE TRIGGER build_physical_identity BEFORE INSERT OR UPDATE ON build_jobs
    FOR EACH ROW EXECUTE FUNCTION bind_build_physical_identity();
CREATE UNIQUE INDEX build_jobs_one_active_physical_artifact
    ON build_jobs((COALESCE(physical_derivation_id,derivation_id)))
    WHERE status IN ('queued','building','cancelling');

-- A certified B generation can consume A's exact scan without changing A's
-- immutable scan/source pair. Only enforced flat aliases collapse ambiguity;
-- two different drvs with the same output still have two physical identities.
-- The view's commit is scan provenance, NEVER inferred runtime Git provenance.
CREATE OR REPLACE VIEW view_current_cve_authority AS
WITH latest AS (
    SELECT system.id AS system_id,system.environment_id,system.flake_id,
           COALESCE(NULLIF(btrim(system.system_configuration_name),''),system.hostname) AS configuration_name,
           state.generation,state.store_path,state.generation_matches_current_store_path
    FROM systems system
    JOIN LATERAL (
        SELECT candidate.generation,candidate.store_path,candidate.generation_matches_current_store_path
        FROM view_agent_current_system_states candidate WHERE candidate.hostname=system.hostname
        ORDER BY candidate.timestamp DESC NULLS LAST,candidate.id DESC LIMIT 1
    ) state ON true WHERE system.is_active AND system.management_mode='managed'
), matching AS (
    SELECT observation.*,d.id AS derivation_id,
           COALESCE(d.reused_from_derivation_id,d.id) AS physical_derivation_id
    FROM latest observation
    JOIN commits c ON c.flake_id=observation.flake_id
    JOIN derivations d ON d.commit_id=c.id AND d.derivation_type='nixos'
      AND d.derivation_name=observation.configuration_name
      AND COALESCE(d.store_path,d.expected_store_path)=observation.store_path
    WHERE observation.generation IS NOT NULL
      AND btrim(COALESCE(observation.store_path,''))<>''
      AND observation.generation_matches_current_store_path IS TRUE
), unique_output AS (
    SELECT system_id,environment_id,generation,store_path
    FROM matching GROUP BY system_id,environment_id,generation,store_path
    HAVING count(DISTINCT physical_derivation_id)=1
)
SELECT output.system_id,output.environment_id,output.generation,output.store_path,
       scan.derivation_id,source.commit_id,scan.id AS scan_id,scan.completed_at AS scan_completed_at,
       retained.id AS generation_snapshot_id,scan.scanner_name,scan.scanner_version
FROM unique_output output
JOIN LATERAL (
    SELECT candidate.* FROM cve_scans candidate
    WHERE candidate.status='completed' AND candidate.completed_at IS NOT NULL
      AND candidate.evidence_schema_version=1
      AND EXISTS (SELECT 1 FROM matching target
                  WHERE target.system_id=output.system_id
                    AND (target.derivation_id=candidate.derivation_id
                         OR derivations_share_evidence_scope(target.derivation_id,candidate.derivation_id)))
    ORDER BY candidate.completed_at DESC,candidate.id DESC LIMIT 1
) scan ON true
JOIN derivations source ON source.id=scan.derivation_id
LEFT JOIN evaluation_generation_snapshots retained ON retained.system_id=output.system_id
  AND retained.generation=output.generation AND retained.source_store_path=output.store_path
  AND (retained.derivation_id=scan.derivation_id
       OR derivations_share_evidence_scope(retained.derivation_id,scan.derivation_id))
  AND retained.lineage_verified
  AND evaluation_deployment_artifact_matches(retained.snapshot_id,retained.derivation_id);

COMMENT ON VIEW view_current_cve_authority IS
    'Managed exact observed output, one enforced physical artifact, and actual immutable scan provenance. Supplemental retained configuration lineage may be a proven alias. Neither the selected head nor scan origin establishes unknown runtime Git revision.';

-- Keep the immutable (scan UUID, order, actual derivation) source FK. Replace
-- only the assumption that a configuration assessment executed its own scan.
ALTER TABLE composite_policy_rule_results
    DROP CONSTRAINT composite_policy_rule_results_assessment_derivation_fk;
CREATE FUNCTION guard_composite_reused_scan_source() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE
    target_id integer;
BEGIN
    IF NEW.source_scan_derivation_id IS NULL THEN RETURN NEW; END IF;
    SELECT derivation_id INTO target_id FROM composite_policy_assessments
        WHERE id=NEW.assessment_id;
    IF target_id IS NULL OR NOT (target_id=NEW.source_scan_derivation_id
        OR derivations_share_evidence_scope(target_id,NEW.source_scan_derivation_id)) THEN
        RAISE EXCEPTION 'Composite scan source requires exact configuration artifact proof';
    END IF;
    RETURN NEW;
END;
$$;
CREATE TRIGGER composite_reused_scan_source BEFORE INSERT OR UPDATE ON composite_policy_rule_results
    FOR EACH ROW EXECUTE FUNCTION guard_composite_reused_scan_source();

-- The removed wide FK also prevented moving a scan-backed assessment to a new
-- carrier. Keep that restriction: B needs fresh assessment/policy evaluation,
-- not A's assessment ID with a different target stamped onto it.
CREATE FUNCTION guard_scan_backed_assessment_carrier() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF NEW.derivation_id IS DISTINCT FROM OLD.derivation_id
       AND EXISTS (SELECT 1 FROM composite_policy_rule_results rule
                   WHERE rule.assessment_id=OLD.id AND rule.source_scan_id IS NOT NULL) THEN
        RAISE EXCEPTION 'Scan-backed assessment carrier is immutable';
    END IF;
    RETURN NEW;
END;
$$;
CREATE TRIGGER scan_backed_assessment_carrier BEFORE UPDATE ON composite_policy_assessments
    FOR EACH ROW EXECUTE FUNCTION guard_scan_backed_assessment_carrier();

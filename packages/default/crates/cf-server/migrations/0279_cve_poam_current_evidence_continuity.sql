-- CVE authority is based on the latest observation, not the latest valid
-- observation. Count every registered output candidate before choosing a scan;
-- archived commits are still competitors. A clean sealed scan is authority.
CREATE VIEW view_current_cve_authority AS
WITH latest AS (
    SELECT system.id AS system_id, system.environment_id, system.flake_id,
           COALESCE(NULLIF(btrim(system.system_configuration_name), ''),
                    system.hostname) AS configuration_name,
           state.generation, state.store_path,
           state.generation_matches_current_store_path
    FROM systems system
    JOIN LATERAL (
        SELECT candidate.generation, candidate.store_path,
               candidate.generation_matches_current_store_path
        FROM system_states candidate
        WHERE candidate.hostname=system.hostname
        ORDER BY candidate.timestamp DESC NULLS LAST, candidate.id DESC
        LIMIT 1
    ) state ON true
    WHERE system.is_active
), valid_observation AS (
    SELECT * FROM latest
    WHERE generation IS NOT NULL AND btrim(COALESCE(store_path, '')) <> ''
      AND generation_matches_current_store_path IS TRUE
), unique_output AS (
    SELECT observation.system_id, observation.environment_id,
           observation.generation, observation.store_path,
           min(derivation.id) AS derivation_id,
           min(derivation.commit_id) AS commit_id
    FROM valid_observation observation
    JOIN derivations derivation
      ON derivation.derivation_type='nixos'
     AND derivation.derivation_name=observation.configuration_name
     AND COALESCE(derivation.store_path, derivation.expected_store_path)=
         observation.store_path
    JOIN commits commit ON commit.id=derivation.commit_id
      AND commit.flake_id=observation.flake_id
    GROUP BY observation.system_id, observation.environment_id,
             observation.generation, observation.store_path
    HAVING count(*)=1
)
SELECT output.system_id, output.environment_id, output.generation,
       output.store_path, output.derivation_id, output.commit_id,
       scan.id AS scan_id, scan.completed_at AS scan_completed_at,
       retained.id AS generation_snapshot_id,
       scan.scanner_name, scan.scanner_version
FROM unique_output output
JOIN LATERAL (
    SELECT candidate.id, candidate.completed_at,
           candidate.scanner_name, candidate.scanner_version
    FROM cve_scans candidate
    WHERE candidate.derivation_id=output.derivation_id
      AND candidate.status='completed'
      AND candidate.completed_at IS NOT NULL
      AND candidate.evidence_schema_version=1
    ORDER BY candidate.completed_at DESC, candidate.id DESC
    LIMIT 1
) scan ON true
LEFT JOIN evaluation_generation_snapshots retained
  ON retained.system_id=output.system_id
 AND retained.generation=output.generation
 AND retained.source_store_path=output.store_path
 AND retained.derivation_id=output.derivation_id
 AND retained.commit_id=output.commit_id
 AND retained.lineage_verified
 AND EXISTS (
     SELECT 1 FROM evaluation_snapshots artifact
     JOIN derivations derivation ON derivation.id=retained.derivation_id
     WHERE artifact.id=retained.snapshot_id
       AND artifact.commit_id=retained.commit_id
       AND artifact.configuration_name=retained.configuration_name
        AND artifact.lifecycle='available' AND artifact.schema_version=1
        AND artifact.integrity_version=1
       AND derivation.commit_id=retained.commit_id
       AND derivation.derivation_name=retained.configuration_name
       AND derivation.derivation_type='nixos'
       AND COALESCE(derivation.store_path, derivation.expected_store_path)=
           retained.source_store_path
 );

COMMENT ON VIEW view_current_cve_authority IS
    'One latest consistent observed generation, unique registered NixOS output and newest completed schema-1 scan per active system. Retained evaluation lineage is supplemental, not required for CVE authority.';

-- Preserve the existing column order and types for dependent dashboard views.
CREATE OR REPLACE VIEW view_current_exact_cve_occurrences AS
SELECT DISTINCT ON (authority.system_id, observation.canonical_cve_id,
                    observation.canonical_package_name)
       authority.system_id, authority.environment_id,
       environment.name AS environment_name,
       authority.scan_id, authority.scan_completed_at AS completed_at,
       observation.canonical_cve_id AS cve_id,
       observation.canonical_package_name AS package_name,
       observation.observed_package_version AS installed_version,
       observation.observed_derivation_path
FROM view_current_cve_authority authority
LEFT JOIN environments environment ON environment.id=authority.environment_id
JOIN cve_scan_vulnerability_observations observation
  ON observation.scan_id=authority.scan_id AND NOT observation.is_whitelisted
ORDER BY authority.system_id, observation.canonical_cve_id,
         observation.canonical_package_name,
         observation.observed_derivation_path;

COMMENT ON VIEW view_current_exact_cve_occurrences IS
    'Current non-whitelisted exact CVE/package pairs from the unique running output newest sealed schema-1 scan; deterministic package-path representative, without a retained-artifact requirement.';

ALTER TABLE poam_cve_finding_links
    ALTER COLUMN baseline_generation_snapshot_id DROP NOT NULL;
ALTER TABLE poam_cve_verification_items
    ALTER COLUMN baseline_generation_snapshot_id DROP NOT NULL;

-- The first anonymous CHECK from 0259 requires a retained snapshot for every
-- non-missing result. Replace only that CHECK, keeping all other result and
-- occurrence checks and the restrictive foreign keys intact.
DO $$
DECLARE v_constraint text;
BEGIN
    SELECT conname INTO STRICT v_constraint
    FROM pg_constraint
    WHERE conrelid='poam_cve_verification_items'::regclass
      AND contype='c'
      AND pg_get_constraintdef(oid) LIKE '%generation_snapshot_id IS NOT NULL%';
    EXECUTE format('ALTER TABLE poam_cve_verification_items DROP CONSTRAINT %I',
                   v_constraint);
END;
$$;
ALTER TABLE poam_cve_verification_items
    ADD CONSTRAINT poam_cve_verification_current_evidence_check CHECK (
        (result='missing' AND scan_id IS NULL
            AND scan_derivation_id IS NULL AND scan_completed_at IS NULL
            AND generation_snapshot_id IS NULL AND generation IS NULL
            AND target_store_path IS NULL)
        OR (result<>'missing' AND scan_id IS NOT NULL
            AND scan_derivation_id IS NOT NULL AND scan_completed_at IS NOT NULL
            AND generation IS NOT NULL
            AND btrim(COALESCE(target_store_path, ''))<>'')
    );

COMMENT ON TABLE poam_cve_finding_links IS
    'Append-only CVE remediation links retain the exact link-time scan, occurrence, generation and store path. A retained evaluation-generation snapshot is optional supplemental provenance.';
COMMENT ON COLUMN poam_cve_finding_links.baseline_generation_snapshot_id IS
    'Optional retained evaluation lineage for immutable link-time CVE evidence; absence does not weaken exact scan and occurrence validation.';
COMMENT ON COLUMN poam_cve_verification_items.baseline_generation_snapshot_id IS
    'Immutable copy of the link optional retained-generation baseline; NULL must match NULL.';
COMMENT ON COLUMN poam_cve_verification_items.generation_snapshot_id IS
    'Optional retained evaluation lineage for the current exact CVE scan; a non-NULL reference must match the current output and available integrity-1 artifact.';

-- CONCURRENCY: Keep global CVE, system sentinel, then exact finding lock order.
-- A retired link may be reinserted only with the same complete historical
-- identity for the same POA&M; this does not rewrite its immutable baseline.
CREATE OR REPLACE FUNCTION protect_poam_cve_finding_link_history()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP = 'INSERT' THEN
        PERFORM lock_poam_cve_key(NEW.canonical_cve_id);
        PERFORM lock_poam_finding_key(NEW.system_id, '00000000-0000-0000-0000-000000000000');
        PERFORM lock_poam_cve_finding_key(
            NEW.system_id, NEW.canonical_cve_id, NEW.canonical_package_name);
        IF EXISTS (SELECT 1 FROM poams WHERE id=NEW.poam_id AND status='completed') THEN
            RAISE EXCEPTION 'A completed POA&M cannot accept a CVE finding link'
                USING ERRCODE='23514',
                      CONSTRAINT='poams_completed_without_cve_history_additions';
        END IF;
        IF NOT EXISTS (
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
              AND history.baseline_generation_snapshot_id IS NOT DISTINCT FROM
                  NEW.baseline_generation_snapshot_id
              AND history.baseline_generation=NEW.baseline_generation
              AND history.baseline_target_store_path=NEW.baseline_target_store_path
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
    IF TG_OP='DELETE' THEN
        RAISE EXCEPTION 'CVE POA&M finding link history is immutable';
    END IF;
    IF NEW.id<>OLD.id OR NEW.poam_id<>OLD.poam_id
       OR NEW.cve_finding_id<>OLD.cve_finding_id
       OR NEW.system_id<>OLD.system_id
       OR NEW.canonical_cve_id<>OLD.canonical_cve_id
       OR NEW.canonical_package_name<>OLD.canonical_package_name
       OR NEW.baseline_scan_id<>OLD.baseline_scan_id
       OR NEW.baseline_scan_derivation_id<>OLD.baseline_scan_derivation_id
       OR NEW.baseline_scan_completed_at<>OLD.baseline_scan_completed_at
       OR NEW.baseline_generation_snapshot_id IS DISTINCT FROM
          OLD.baseline_generation_snapshot_id
       OR NEW.baseline_generation<>OLD.baseline_generation
       OR NEW.baseline_target_store_path<>OLD.baseline_target_store_path
       OR NEW.baseline_occurrence_derivation_path<>
          OLD.baseline_occurrence_derivation_path
       OR NEW.baseline_observed_package_version<>
          OLD.baseline_observed_package_version
       OR NEW.linked_by<>OLD.linked_by OR NEW.linked_at<>OLD.linked_at THEN
        RAISE EXCEPTION 'CVE POA&M finding link identity is immutable';
    END IF;
    IF OLD.retired_at IS NOT NULL THEN
        RAISE EXCEPTION 'Retired CVE POA&M finding links are immutable';
    END IF;
    IF NEW.retired_at IS NULL OR NEW.retired_by IS NULL
       OR btrim(COALESCE(NEW.retirement_reason, ''))='' THEN
        RAISE EXCEPTION 'A CVE POA&M finding link update must retire the active link';
    END IF;
    RETURN NEW;
END;
$$;

-- Verification copies the immutable baseline even when current evidence is
-- missing. A non-missing result must use the latest exact scan strictly later
-- than the baseline; neither a whitelist nor a justification proves absence.
CREATE OR REPLACE FUNCTION validate_poam_cve_verification_provenance()
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
          AND link.system_id=NEW.system_id
          AND link.canonical_cve_id=NEW.canonical_cve_id
          AND link.canonical_package_name=NEW.canonical_package_name
          AND link.baseline_scan_id=NEW.baseline_scan_id
          AND link.baseline_scan_derivation_id=NEW.baseline_scan_derivation_id
          AND link.baseline_scan_completed_at=NEW.baseline_scan_completed_at
          AND link.baseline_generation_snapshot_id IS NOT DISTINCT FROM
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
    IF NEW.result='missing' THEN RETURN NEW; END IF;
    IF NOT EXISTS (
        SELECT 1 FROM view_current_cve_authority authority
        WHERE authority.system_id=NEW.system_id
          AND authority.scan_id=NEW.scan_id
          AND authority.derivation_id=NEW.scan_derivation_id
          AND authority.scan_completed_at=NEW.scan_completed_at
          AND authority.scan_completed_at>NEW.baseline_scan_completed_at
          AND authority.generation=NEW.generation
          AND authority.store_path=NEW.target_store_path
          AND (NEW.generation_snapshot_id IS NULL
               OR authority.generation_snapshot_id=NEW.generation_snapshot_id)
    ) THEN
        RAISE EXCEPTION 'CVE verification requires a newer sealed exact Current scan'
            USING ERRCODE='23514',
                  CONSTRAINT='poam_cve_verification_authoritative_scan';
    END IF;

    IF NEW.occurrence_present THEN
        SELECT observation.is_whitelisted INTO v_occurrence_whitelisted
        FROM cve_scan_vulnerability_observations observation
        WHERE observation.scan_id=NEW.scan_id
          AND observation.observed_derivation_path=NEW.occurrence_derivation_path
          AND observation.canonical_cve_id=NEW.canonical_cve_id
          AND observation.canonical_package_name=NEW.canonical_package_name;
        IF NOT FOUND THEN
            RAISE EXCEPTION 'Positive CVE verification requires matching immutable occurrence evidence'
                USING ERRCODE='23514',
                      CONSTRAINT='poam_cve_verification_occurrence_required';
        END IF;
        SELECT EXISTS (
            SELECT 1 FROM system_cve_justifications justification
            WHERE justification.cve_id=NEW.canonical_cve_id
              AND (justification.system_id IS NULL
                   OR justification.system_id=NEW.system_id)
        ) INTO v_justified;
    ELSIF NEW.result='pass' AND EXISTS (
        SELECT 1 FROM cve_scan_vulnerability_observations observation
        WHERE observation.scan_id=NEW.scan_id
          AND observation.canonical_cve_id=NEW.canonical_cve_id
          AND observation.canonical_package_name=NEW.canonical_package_name
    ) THEN
        RAISE EXCEPTION 'CVE PASS or absence result conflicts with scan occurrence evidence'
            USING ERRCODE='23514',
                  CONSTRAINT='poam_cve_verification_occurrence_absent';
    END IF;
    IF NEW.result='fail' AND v_occurrence_whitelisted THEN
        RAISE EXCEPTION 'Unwhitelisted CVE failure cannot cite whitelisted evidence'
            USING ERRCODE='23514',
                  CONSTRAINT='poam_cve_verification_fail_state';
    END IF;
    IF NEW.result='fail' AND v_justified THEN
        RAISE EXCEPTION 'Unjustified CVE failure cannot cite justified evidence'
            USING ERRCODE='23514',
                  CONSTRAINT='poam_cve_verification_fail_justification_state';
    END IF;
    IF NEW.result='whitelisted' AND NOT v_occurrence_whitelisted THEN
        RAISE EXCEPTION 'Whitelisted CVE result requires whitelisted evidence'
            USING ERRCODE='23514',
                  CONSTRAINT='poam_cve_verification_whitelist_state';
    END IF;
    IF NEW.result='justified' AND NOT v_justified THEN
        RAISE EXCEPTION 'Justified CVE result requires an applicable current justification'
            USING ERRCODE='23514',
                  CONSTRAINT='poam_cve_verification_justification_state';
    END IF;
    IF NEW.occurrence_present AND NEW.result<>(CASE
          WHEN v_occurrence_whitelisted THEN 'whitelisted'
          WHEN v_justified THEN 'justified'
          ELSE 'fail' END) THEN
        RAISE EXCEPTION 'Positive CVE verification result does not match current evidence state'
            USING ERRCODE='23514',
                  CONSTRAINT='poam_cve_verification_result_state';
    END IF;
    IF NOT NEW.occurrence_present AND NEW.result<>'pass' THEN
        RAISE EXCEPTION 'Absent CVE occurrence requires a PASS result'
            USING ERRCODE='23514',
                  CONSTRAINT='poam_cve_verification_absence_result';
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
            USING ERRCODE='23514',
                  CONSTRAINT='poam_cve_verification_package_version';
    END IF;
    RETURN NEW;
END;
$$;

-- An environment owns every current affected subject without a host override.
-- Clean and moved-out historical members do not negate that ownership.
CREATE OR REPLACE FUNCTION cve_coherent_environment_disposition_state(
    v_cve_id text, v_package_name text, v_environment_id uuid
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
           AND EXISTS (SELECT 1 FROM users assignee
             WHERE assignee.id=poam.owner_user_id AND assignee.is_active
               AND assignee.user_type='human'))
          OR
          (poam.owner_kind='oidc_group'
           AND poam.owner_user_id IS NULL
           AND poam.owner_group_name IS NOT NULL
           AND btrim(poam.owner_group_name)<>''
           AND EXISTS (SELECT 1 FROM oidc_group_mappings mapping
             WHERE mapping.group_name=poam.owner_group_name))))
    AND NOT EXISTS (
      SELECT 1 FROM view_current_exact_cve_occurrences subject
      WHERE subject.cve_id=v_cve_id
        AND subject.package_name=v_package_name
        AND subject.environment_id=v_environment_id
        AND NOT EXISTS (SELECT 1 FROM cve_current_system_dispositions host
          WHERE host.system_id=subject.system_id
            AND host.canonical_cve_id=v_cve_id
            AND host.canonical_package_name=v_package_name)
        AND NOT EXISTS (
          SELECT 1 FROM poam_cve_finding_links link
          WHERE link.poam_id=disposition.poam_id
            AND link.system_id=subject.system_id
            AND link.canonical_cve_id=v_cve_id
            AND link.canonical_package_name=v_package_name
            AND link.retired_at IS NULL))
    THEN 'scheduled'
  ELSE NULL
END
FROM cve_current_environment_dispositions disposition
WHERE disposition.canonical_cve_id=v_cve_id
  AND disposition.canonical_package_name=v_package_name
  AND disposition.environment_id=v_environment_id
$$;

COMMENT ON FUNCTION cve_coherent_environment_disposition_state(text,text,uuid) IS
    'Returns ACCEPTED directly. SCHEDULED requires a live owned POA&M covering every current exact affected subject without a host override; extra historical members remain valid.';

-- SECURITY: Closure independently re-resolves the newest Current scan after
-- verification. The policy branch below is unchanged from migration 0259.
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
              AND baseline_link.system_id=item.system_id
              AND baseline_link.canonical_cve_id=item.canonical_cve_id
              AND baseline_link.canonical_package_name=item.canonical_package_name
              AND baseline_link.baseline_scan_id=item.baseline_scan_id
              AND baseline_link.baseline_scan_derivation_id=
                  item.baseline_scan_derivation_id
              AND baseline_link.baseline_scan_completed_at=
                  item.baseline_scan_completed_at
              AND baseline_link.baseline_generation_snapshot_id
                  IS NOT DISTINCT FROM item.baseline_generation_snapshot_id
              AND baseline_link.baseline_generation=item.baseline_generation
              AND baseline_link.baseline_target_store_path=
                  item.baseline_target_store_path
              AND baseline_link.baseline_occurrence_derivation_path=
                  item.baseline_occurrence_derivation_path
              AND baseline_link.baseline_observed_package_version=
                  item.baseline_observed_package_version)
          OR NOT EXISTS (
            SELECT 1 FROM view_current_cve_authority authority
            WHERE authority.system_id=item.system_id
              AND authority.scan_id=item.scan_id
              AND authority.derivation_id=item.scan_derivation_id
              AND authority.scan_completed_at=item.scan_completed_at
              AND authority.scan_completed_at>item.baseline_scan_completed_at
              AND authority.generation=item.generation
              AND authority.store_path=item.target_store_path
              AND (item.generation_snapshot_id IS NULL
                   OR authority.generation_snapshot_id=
                      item.generation_snapshot_id)
              AND NOT EXISTS (
                SELECT 1 FROM cve_scan_vulnerability_observations observation
                WHERE observation.scan_id=authority.scan_id
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

COMMENT ON FUNCTION protect_poam_cve_finding_link_history() IS
    'Validates an exact latest Current CVE occurrence and optional available retained lineage at insert. Reopening an identical retired link preserves its previously validated immutable baseline; retirement is the only update.';
COMMENT ON FUNCTION validate_poam_cve_verification_provenance() IS
    'Copies the immutable link baseline, including nullable retained provenance. Non-missing results require the newest exact Current scan strictly later than baseline; PASS requires absence even of whitelisted occurrences.';
COMMENT ON FUNCTION validate_poam_closure_evidence() IS
    'Preserves policy closure validation. Exact CVE closure independently requires copied baselines, PASS for every closed finding, and unchanged newer latest Current scan absence at closure time.';

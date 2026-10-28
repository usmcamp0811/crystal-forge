-- A source-neutral Pass can close a POA&M without a composite assessment, so
-- the database must validate that evidence against authoritative source rows.
-- Comparing a verification item only with its own snapshot permits a direct
-- writer to invent both values. The closure constraint below instead requires
-- the latest deployed store path, its exact NixOS derivation, and the source row
-- that produced the Pass. The service obtains policy version, effective config,
-- and effective-set digest from the authoritative resolver in the same locked
-- transaction. SQL intentionally does not implement a second policy resolver:
-- doing so would diverge for direct policies, environment and system assignment
-- precedence, additions, exclusions, overrides, conflicts, and pinned history.

CREATE TABLE poam_effective_context_attestations (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    attempt_id uuid NOT NULL REFERENCES poam_verification_attempts(id) ON DELETE RESTRICT,
    finding_id uuid NOT NULL,
    system_id uuid NOT NULL,
    policy_lineage_id uuid NOT NULL,
    policy_version_id uuid NOT NULL,
    derivation_id integer NOT NULL,
    target_store_path text NOT NULL,
    effective_set_digest text NOT NULL,
    effective_config_digest text NOT NULL,
    effective_config jsonb NOT NULL,
    observed_outcome text NOT NULL CHECK (observed_outcome IN ('pass', 'fail')),
    observation_token text NOT NULL,
    observation_snapshot jsonb NOT NULL,
    created_at timestamptz NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE (attempt_id, finding_id),
    FOREIGN KEY (finding_id, system_id, policy_lineage_id)
        REFERENCES poam_findings(id, system_id, policy_lineage_id) ON DELETE RESTRICT
);

ALTER TABLE poam_verification_items
    ADD COLUMN effective_context_attestation_id uuid REFERENCES poam_effective_context_attestations(id) ON DELETE RESTRICT;

CREATE OR REPLACE FUNCTION protect_poam_effective_context_attestation()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
DECLARE
    v_sealed_at timestamptz;
BEGIN
    IF TG_OP <> 'INSERT' THEN
        RAISE EXCEPTION 'POA&M effective-context attestations are immutable';
    END IF;
    SELECT sealed_at INTO v_sealed_at
    FROM poam_verification_attempts
    WHERE id=NEW.attempt_id
    FOR UPDATE;
    IF v_sealed_at IS NOT NULL THEN
        RAISE EXCEPTION 'A sealed POA&M attempt cannot accept an attestation';
    END IF;
    RETURN NEW;
END;
$$;
CREATE TRIGGER trigger_protect_poam_effective_context_attestation
    BEFORE INSERT OR UPDATE OR DELETE ON poam_effective_context_attestations
    FOR EACH ROW EXECUTE FUNCTION protect_poam_effective_context_attestation();

-- Rust observation tokens hash recursively key-sorted compact JSON. jsonb
-- already normalizes object keys, but its text representation includes
-- separator whitespace. This serializer reproduces the compact representation
-- without changing whitespace that is part of a JSON string value.
CREATE OR REPLACE FUNCTION canonical_poam_observation_json(value jsonb)
RETURNS text
LANGUAGE sql
IMMUTABLE
STRICT
PARALLEL SAFE
AS $$
    SELECT CASE jsonb_typeof(value)
        WHEN 'object' THEN (
            SELECT '{' || COALESCE(string_agg(
                to_jsonb(entry.key)::text || ':' ||
                    canonical_poam_observation_json(entry.value),
                ',' ORDER BY entry.key
            ), '') || '}'
            FROM jsonb_each(value) AS entry
        )
        WHEN 'array' THEN (
            SELECT '[' || COALESCE(string_agg(
                canonical_poam_observation_json(element.value),
                ',' ORDER BY element.ordinality
            ), '') || ']'
            FROM jsonb_array_elements(value) WITH ORDINALITY AS element(value, ordinality)
        )
        ELSE value::text
    END
$$;

CREATE OR REPLACE FUNCTION validate_poam_closure_evidence()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    -- INVARIANT: A completed POA&M contains one sealed accepted item for each
    -- retired finding. Composite Pass and waiver evidence retain their existing
    -- validation. A legacy Pass additionally has to be reproducible from rows
    -- outside POA&M history at constraint-check time. This prevents a direct
    -- database writer from inventing a self-consistent source-neutral snapshot.
    IF NEW.status = 'completed' AND (
        NOT EXISTS (
            SELECT 1 FROM poam_verification_attempts attempt
            WHERE attempt.id=NEW.closure_attempt_id AND attempt.poam_id=NEW.id
              AND attempt.outcome='accepted' AND attempt.sealed_at IS NOT NULL
        )
        OR NOT EXISTS (
            SELECT 1 FROM poam_verification_items item
            WHERE item.attempt_id=NEW.closure_attempt_id
        )
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
                         SELECT 1
                         FROM composite_policy_assessments assessment
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
                               canonical_poam_observation_json(item.observation_snapshot),
                               'sha256'
                           ),'hex')
                           AND item.observation_snapshot=jsonb_build_object(
                               'assessment',to_jsonb(assessment),
                               'rules',COALESCE((
                                   SELECT jsonb_agg(to_jsonb(rule_result)
                                       ORDER BY rule_result.ordinal,rule_result.rule_id)
                                   FROM composite_policy_rule_results rule_result
                                   WHERE rule_result.assessment_id=assessment.id
                               ),'[]'::jsonb)
                           )
                     ))
                    OR
                    (item.assessment_id IS NULL
                     AND item.effective_context_attestation_id IS NOT NULL
                     AND EXISTS (
                         SELECT 1 FROM poam_effective_context_attestations attestation
                         WHERE attestation.id=item.effective_context_attestation_id
                           AND attestation.attempt_id=item.attempt_id
                           AND attestation.finding_id=item.finding_id
                           AND attestation.system_id=item.system_id
                           AND attestation.policy_lineage_id=item.policy_lineage_id
                           AND attestation.policy_version_id=item.policy_version_id
                           AND attestation.derivation_id=item.derivation_id
                           AND attestation.target_store_path=item.target_store_path
                           AND attestation.effective_set_digest=item.effective_set_digest
                           AND attestation.effective_config_digest=item.effective_config_digest
                           AND attestation.effective_config=item.effective_config
                           AND attestation.observed_outcome='pass'
                           AND attestation.observation_token=item.observation_token
                           AND attestation.observation_snapshot=item.observation_snapshot
                     )
                     AND item.observation_token=encode(digest(
                         canonical_poam_observation_json(item.observation_snapshot),
                         'sha256'
                     ),'hex')
                     AND item.effective_config_digest=encode(digest(
                         canonical_poam_observation_json(item.effective_config),
                         'sha256'
                     ),'hex')
                     AND EXISTS (
                        SELECT 1
                        FROM systems system
                        JOIN LATERAL (
                            SELECT state.store_path,state.timestamp
                            FROM system_states state
                            WHERE state.hostname=system.hostname
                              AND state.store_path IS NOT NULL
                              AND btrim(state.store_path)<>''
                            ORDER BY state.timestamp DESC,state.id DESC
                            LIMIT 1
                        ) deployed ON deployed.store_path=item.target_store_path
                        JOIN derivations derivation
                          ON derivation.id=item.derivation_id
                         AND derivation.derivation_type='nixos'
                         AND COALESCE(derivation.store_path,derivation.expected_store_path)
                             =deployed.store_path
                         JOIN deployment_policy_versions policy_version
                           ON policy_version.id=item.policy_version_id
                          AND policy_version.policy_id=item.policy_lineage_id
                        WHERE system.id=item.system_id
                          AND item.assessment_updated_at=deployed.timestamp
                          AND (
                            (policy_version.policy_type IN (
                                'require_packages','custom_check','require_cf_agent'
                             )
                             AND derivation.policy_results->'assigned'
                                   ->item.policy_version_id::text->>'passed'='true'
                             AND jsonb_typeof(derivation.policy_results->'assigned'
                                   ->item.policy_version_id::text)='object'
                             AND (
                                NOT (derivation.policy_results->'assigned'
                                    ->item.policy_version_id::text ? 'details')
                                OR jsonb_typeof(derivation.policy_results->'assigned'
                                    ->item.policy_version_id::text->'details')
                                    IN ('string','null')
                             )
                             AND item.observation_snapshot=jsonb_build_object(
                                'source','nix_policy_result',
                                'system_id',item.system_id,
                                'policy_lineage_id',item.policy_lineage_id,
                                'policy_version_id',item.policy_version_id,
                                'effective_set_digest',item.effective_set_digest,
                                'effective_config_digest',item.effective_config_digest,
                                'derivation_id',item.derivation_id,
                                'target_store_path',item.target_store_path,
                                'passed',true,
                                'details',derivation.policy_results->'assigned'
                                    ->item.policy_version_id::text->'details'
                             ))
                            OR
                            (policy_version.policy_type='require_cve_check'
                             AND EXISTS (
                                SELECT 1
                                FROM cve_scans scan
                                WHERE scan.id=(item.observation_snapshot->>'scan_id')::uuid
                                  AND scan.derivation_id=item.derivation_id
                                  AND scan.status='completed'
                                  AND scan.id=(
                                    SELECT current_scan.id FROM cve_scans current_scan
                                    WHERE current_scan.derivation_id=item.derivation_id
                                      AND current_scan.status='completed'
                                    ORDER BY current_scan.completed_at DESC NULLS LAST,
                                             current_scan.id DESC
                                    LIMIT 1
                                  )
                                   AND scan.critical_count<=COALESCE(
                                       (item.effective_config->>'max_critical')::bigint,
                                      9223372036854775807
                                  )
                                  AND (
                                       item.effective_config->>'max_high' IS NULL
                                       OR scan.high_count<=(item.effective_config->>'max_high')::bigint
                                  )
                                  AND item.observation_snapshot=jsonb_build_object(
                                    'source','cve_scan',
                                    'system_id',item.system_id,
                                    'policy_lineage_id',item.policy_lineage_id,
                                    'policy_version_id',item.policy_version_id,
                                    'effective_set_digest',item.effective_set_digest,
                                    'effective_config_digest',item.effective_config_digest,
                                    'derivation_id',item.derivation_id,
                                    'target_store_path',item.target_store_path,
                                    'scan_id',scan.id,
                                    'critical_count',scan.critical_count,
                                    'high_count',scan.high_count,
                                    'max_critical',COALESCE(
                                         (item.effective_config->>'max_critical')::bigint,
                                        9223372036854775807
                                    ),
                                     'max_high',(item.effective_config->>'max_high')::bigint
                                  )
                             ))
                          )
                     ))
                 ))
                OR
                (item.result='waiver' AND item.assessment_id IS NOT NULL
                 AND item.observed_outcome='fail'
                 AND item.observation_snapshot->'assessment'->>'overall_outcome'='fail'
                 AND EXISTS (
                    SELECT 1 FROM finding_waivers waiver
                    WHERE waiver.id=item.waiver_id AND waiver.finding_id=item.finding_id
                      AND waiver.assessment_id=item.assessment_id
                      AND waiver.policy_version_id=item.policy_version_id
                      AND waiver.observation_token=item.observation_token
                      AND waiver.observation_snapshot=item.observation_snapshot
                      AND waiver.status='accepted' AND waiver.accepted_at<=CURRENT_TIMESTAMP
                      AND (waiver.expires_at IS NULL OR waiver.expires_at>CURRENT_TIMESTAMP)
                 ))
              )
        )
    ) THEN
        RAISE EXCEPTION 'Completed POA&M closure evidence is incomplete or inconsistent'
            USING ERRCODE='23514', CONSTRAINT='poams_authoritative_closure_evidence';
    END IF;
    RETURN NEW;
END;
$$;

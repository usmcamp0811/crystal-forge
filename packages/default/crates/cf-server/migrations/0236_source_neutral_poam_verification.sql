-- Permit canonical POA&M closure evidence from deployed legacy observations.
-- Composite assessment evidence keeps its existing shape and validation.

ALTER TABLE poam_verification_items
    DROP CONSTRAINT poam_verification_items_check,
    DROP CONSTRAINT poam_verification_items_check1;

ALTER TABLE poam_verification_items
    ADD CONSTRAINT poam_verification_items_accepted_evidence CHECK (
        result NOT IN ('pass', 'waiver') OR (
            policy_version_id IS NOT NULL
            AND derivation_id IS NOT NULL
            AND target_store_path IS NOT NULL
            AND effective_set_digest IS NOT NULL
            AND effective_config_digest IS NOT NULL
            AND effective_config IS NOT NULL
            AND observed_outcome IS NOT NULL
            AND observation_token IS NOT NULL
            AND observation_snapshot IS NOT NULL
            AND assessment_updated_at IS NOT NULL
            AND (assessment_id IS NOT NULL OR result = 'pass')
        )
    ),
    ADD CONSTRAINT poam_verification_items_observation_shape CHECK (
        (observation_snapshot IS NULL AND assessment_updated_at IS NULL)
        OR (observation_snapshot IS NOT NULL AND assessment_updated_at IS NOT NULL)
    );

CREATE OR REPLACE FUNCTION validate_poam_closure_evidence()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
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
                     AND item.observation_snapshot->'assessment'->>'id'=item.assessment_id::text
                     AND item.observation_snapshot->'assessment'->>'system_id'=item.system_id::text
                     AND item.observation_snapshot->'assessment'->>'policy_lineage_id'=item.policy_lineage_id::text
                     AND item.observation_snapshot->'assessment'->>'policy_version_id'=item.policy_version_id::text
                     AND item.observation_snapshot->'assessment'->>'overall_outcome'='pass')
                    OR
                    (item.assessment_id IS NULL
                     AND item.observation_snapshot->>'system_id'=item.system_id::text
                     AND item.observation_snapshot->>'policy_lineage_id'=item.policy_lineage_id::text
                     AND item.observation_snapshot->>'policy_version_id'=item.policy_version_id::text
                     AND item.observation_snapshot->>'derivation_id'=item.derivation_id::text
                     AND item.observation_snapshot->>'target_store_path'=item.target_store_path
                     AND item.observation_snapshot->>'effective_set_digest'=item.effective_set_digest
                     AND item.observation_snapshot->>'effective_config_digest'=item.effective_config_digest
                     AND (
                        (item.observation_snapshot->>'source'='nix_policy_result'
                         AND (item.observation_snapshot->>'passed')::boolean)
                        OR
                        (item.observation_snapshot->>'source'='cve_scan'
                         AND (item.observation_snapshot->>'critical_count')::bigint
                             <= (item.observation_snapshot->>'max_critical')::bigint
                         AND (item.observation_snapshot->>'max_high' IS NULL
                              OR (item.observation_snapshot->>'high_count')::bigint
                                 <= (item.observation_snapshot->>'max_high')::bigint))
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

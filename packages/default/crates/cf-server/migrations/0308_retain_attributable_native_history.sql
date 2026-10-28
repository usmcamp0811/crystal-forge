-- Preserve facts already attributable before the immutable native writer was
-- installed. No current catalog, artifact-cache inventory, or inferred outcome
-- participates. Ambiguous eligible carriers remain unavailable permanently.
ALTER TABLE evaluation_native_outcomes
    DROP CONSTRAINT evaluation_native_outcomes_eval_state_check;
ALTER TABLE evaluation_native_outcomes
    ADD CONSTRAINT evaluation_native_outcomes_eval_state_check
    CHECK (eval_state IN ('evaluated', 'eval_failed', 'unavailable'));
ALTER TABLE evaluation_native_outcomes
    ADD CONSTRAINT evaluation_native_outcomes_unavailable_gates_check
    CHECK (eval_state <> 'unavailable' OR (cf_agent_enabled IS NULL
        AND policy_requirements_met IS NULL AND evaluation_failure_error IS NULL));
ALTER TABLE evaluation_native_outcomes
    ADD CONSTRAINT evaluation_native_outcomes_native_source_mode_check
    CHECK (lower(COALESCE(policy_results->>'execution_mode','real')) <> 'mock');

WITH marked AS (
    SELECT d.id AS carrier_id,d.commit_id,d.derivation_name,d.status_id,
        d.cf_agent_enabled,d.policy_requirements_met,d.policy_results,d.error_message,
        a.id AS owner_id, a.attempt_number AS owner_number,
        COALESCE(d.policy_results->'evaluation_failure' = 'true'::jsonb
            AND jsonb_typeof(d.policy_results->'evaluation_failure_error') = 'string'
            AND btrim(d.policy_results->>'evaluation_failure_error') <> '', false)
            AS confirmed_failure
    FROM derivations d
    JOIN evaluation_attempts a ON a.commit_id=d.commit_id
        AND d.policy_results->'evaluation_attempt'=to_jsonb(a.attempt_number)
    JOIN evaluation_attempt_inventory i ON i.evaluation_attempt_id=a.id
        AND i.commit_id=a.commit_id AND i.attempt_number=a.attempt_number
        AND i.execution_mode='native'
        AND d.derivation_name=ANY(i.selected_configuration_names)
    WHERE d.derivation_type='nixos'
        -- Terminal configuration facts can precede overall attempt completion.
        -- Native inventory plus the exact marker supplies their ownership.
        AND (d.policy_results->>'evaluation_attempt') ~ '^[1-9][0-9]*$'
        AND lower(COALESCE(d.policy_results->>'execution_mode','real')) <> 'mock'
), eligible AS (
    SELECT *, count(*) OVER (PARTITION BY owner_id,derivation_name) AS carriers,
        row_number() OVER (PARTITION BY owner_id,derivation_name ORDER BY carrier_id) AS ordinal
    FROM marked
    WHERE (status_id IN (5,6,7,8,10,12) OR confirmed_failure)
        AND (confirmed_failure OR NOT (policy_results ? 'evaluation_failure'
            OR policy_results ? 'evaluation_failure_error'))
)
INSERT INTO evaluation_native_outcomes(evaluation_attempt_id,commit_id,attempt_number,
    configuration_name,cf_agent_enabled,policy_requirements_met,policy_results,
    eval_state,evaluation_failure_error)
SELECT owner_id,commit_id,owner_number,derivation_name,
    CASE WHEN carriers=1 AND status_id<>6 AND NOT confirmed_failure
        THEN cf_agent_enabled ELSE NULL END,
    CASE WHEN carriers=1 AND status_id<>6 AND NOT confirmed_failure
        THEN policy_requirements_met ELSE NULL END,
    CASE WHEN carriers=1 THEN policy_results
        ELSE jsonb_build_object('evaluation_attempt',owner_number) END,
    CASE WHEN carriers<>1 THEN 'unavailable'
        WHEN status_id=6 OR confirmed_failure THEN 'eval_failed' ELSE 'evaluated' END,
    CASE WHEN carriers=1 THEN CASE WHEN confirmed_failure
        THEN policy_results->>'evaluation_failure_error'
        WHEN status_id=6 THEN error_message ELSE NULL END ELSE NULL END
FROM eligible WHERE ordinal=1
ON CONFLICT (evaluation_attempt_id,configuration_name) DO NOTHING;

COMMENT ON TABLE evaluation_native_outcomes IS
    'Immutable exact-attempt native evidence, including explicit unavailable legacy ambiguity. No runnable artifact identity or admission authority.';

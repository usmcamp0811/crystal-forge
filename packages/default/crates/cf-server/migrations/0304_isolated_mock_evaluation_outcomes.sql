-- Mock evidence has no executable artifact identity. Never join this table into
-- derivation/build admission. The composite FK proves all three owner fields.
ALTER TABLE evaluation_attempts
    ADD CONSTRAINT evaluation_attempts_mock_owner_key
    UNIQUE (id, commit_id, attempt_number);

CREATE TABLE evaluation_mock_outcomes (
    evaluation_attempt_id uuid NOT NULL,
    commit_id integer NOT NULL REFERENCES commits(id) ON DELETE CASCADE,
    attempt_number integer NOT NULL CHECK (attempt_number > 0),
    configuration_name text NOT NULL CHECK (btrim(configuration_name) <> ''),
    policy_results jsonb NOT NULL CHECK (
        jsonb_typeof(policy_results) = 'object'
        AND COALESCE(policy_results->'evaluation_attempt' = to_jsonb(attempt_number), false)
        AND COALESCE(policy_results->>'execution_mode' = 'mock', false)
    ),
    cf_agent_enabled boolean,
    policy_requirements_met boolean NOT NULL,
    captured_at timestamptz NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (evaluation_attempt_id, configuration_name),
    FOREIGN KEY (evaluation_attempt_id, commit_id, attempt_number)
        REFERENCES evaluation_attempts(id, commit_id, attempt_number) ON DELETE CASCADE
);

CREATE INDEX evaluation_mock_outcomes_commit_attempt_idx
    ON evaluation_mock_outcomes(commit_id, attempt_number);

CREATE FUNCTION guard_evaluation_mock_outcome() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP = 'DELETE' THEN
        IF NOT EXISTS (SELECT 1 FROM commits WHERE id = OLD.commit_id)
           OR NOT EXISTS (SELECT 1 FROM evaluation_attempts WHERE id = OLD.evaluation_attempt_id) THEN
            RETURN OLD;
        END IF;
    END IF;
    RAISE EXCEPTION 'Mock evaluation outcomes are first-write immutable';
END;
$$;

CREATE TRIGGER evaluation_mock_outcome_guard BEFORE UPDATE OR DELETE
    ON evaluation_mock_outcomes FOR EACH ROW EXECUTE FUNCTION guard_evaluation_mock_outcome();

COMMENT ON TABLE evaluation_mock_outcomes IS
    'Synthetic presentation evidence only. First-write immutable per exact attempt/configuration; never runnable artifacts or deployment evidence.';

-- This view supplies overrides only when the represented attempt has mock
-- evidence, or a queued retry follows mock evidence. A queued retry must not
-- inherit old mock counts even though commits.evaluation_attempt_count still
-- refers to the last started attempt. Other native metrics keep their old SQL.
CREATE VIEW view_evaluation_mock_counts AS
WITH latest AS (
    SELECT DISTINCT ON (commit_id) id, commit_id, attempt_number, status
    FROM evaluation_attempts ORDER BY commit_id, attempt_number DESC, created_at DESC
), current_mock AS (
    SELECT m.* FROM evaluation_mock_outcomes m
    JOIN latest a ON a.id = m.evaluation_attempt_id
        AND a.commit_id = m.commit_id AND a.attempt_number = m.attempt_number
), unambiguous AS (
    SELECT m.* FROM current_mock m
    WHERE NOT EXISTS (
        SELECT 1 FROM derivations d
        WHERE d.commit_id = m.commit_id AND d.derivation_type = 'nixos'
          AND d.derivation_name = m.configuration_name
          AND d.status_id IN (5, 6, 7, 8, 10, 12)
          AND d.policy_results->'evaluation_attempt' = to_jsonb(m.attempt_number)
    )
)
SELECT a.commit_id, a.id AS evaluation_attempt_id, a.attempt_number,
    (SELECT COUNT(*) FROM unambiguous m WHERE m.evaluation_attempt_id = a.id
        AND m.cf_agent_enabled IS TRUE AND m.policy_requirements_met IS TRUE) AS passed_count,
    (SELECT COUNT(*) FROM unambiguous m WHERE m.evaluation_attempt_id = a.id
        AND (m.cf_agent_enabled IS FALSE OR m.policy_requirements_met IS FALSE)) AS policy_failed_count,
    0::bigint AS eval_failed_count
FROM latest a
WHERE EXISTS (SELECT 1 FROM current_mock m WHERE m.evaluation_attempt_id = a.id)
   OR (a.status = 'queued' AND EXISTS (
       SELECT 1 FROM evaluation_mock_outcomes old WHERE old.commit_id = a.commit_id
   ));

COMMENT ON VIEW view_evaluation_mock_counts IS
    'Queue/history mock-only count overrides for the represented attempt. Ambiguous native/mock configurations contribute no known outcome. Queued retries never inherit old mock counts.';

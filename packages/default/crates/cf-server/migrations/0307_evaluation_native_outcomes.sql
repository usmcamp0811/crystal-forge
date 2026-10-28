-- Historical native evidence only. No runnable artifact identity belongs here.
CREATE TABLE evaluation_native_outcomes (
    evaluation_attempt_id uuid NOT NULL,
    commit_id integer NOT NULL REFERENCES commits(id) ON DELETE CASCADE,
    attempt_number integer NOT NULL CHECK (attempt_number > 0),
    configuration_name text NOT NULL CHECK (btrim(configuration_name) <> ''),
    cf_agent_enabled boolean,
    policy_requirements_met boolean,
    policy_results jsonb NOT NULL CHECK (
        jsonb_typeof(policy_results) = 'object'
        AND jsonb_typeof(policy_results->'evaluation_attempt') = 'number'
        AND (policy_results->>'evaluation_attempt') ~ '^[1-9][0-9]*$'
        AND COALESCE(policy_results->'evaluation_attempt' = to_jsonb(attempt_number), false)
    ),
    eval_state text NOT NULL CHECK (eval_state IN ('evaluated', 'eval_failed')),
    evaluation_failure_error text,
    captured_at timestamptz NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (evaluation_attempt_id, configuration_name),
    FOREIGN KEY (evaluation_attempt_id, commit_id, attempt_number)
        REFERENCES evaluation_attempts(id, commit_id, attempt_number) ON DELETE CASCADE
);

CREATE INDEX evaluation_native_outcomes_commit_attempt_idx
    ON evaluation_native_outcomes(commit_id, attempt_number);

CREATE FUNCTION guard_evaluation_native_outcome() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP = 'DELETE' THEN
        IF NOT EXISTS (SELECT 1 FROM commits WHERE id = OLD.commit_id)
           OR NOT EXISTS (SELECT 1 FROM evaluation_attempts WHERE id = OLD.evaluation_attempt_id) THEN
            RETURN OLD;
        END IF;
    END IF;
    RAISE EXCEPTION 'Native evaluation outcomes are first-write immutable';
END;
$$;

CREATE TRIGGER evaluation_native_outcome_guard BEFORE UPDATE OR DELETE
    ON evaluation_native_outcomes FOR EACH ROW EXECUTE FUNCTION guard_evaluation_native_outcome();

COMMENT ON TABLE evaluation_native_outcomes IS
    'Immutable exact-attempt native terminal evidence for historical presentation. Not an executable artifact source, build admission input, or observation source.';

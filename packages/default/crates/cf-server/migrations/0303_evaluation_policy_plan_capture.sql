-- One first-write public-metadata plan per attempt. System IDs are historical
-- values inside the payload, not FKs: deleting a system remains permitted.
CREATE TABLE evaluation_policy_plans (
    commit_id integer NOT NULL REFERENCES commits(id) ON DELETE CASCADE,
    attempt_number integer NOT NULL CHECK (attempt_number > 0),
    evaluation_attempt_id uuid NOT NULL REFERENCES evaluation_attempts(id) ON DELETE CASCADE,
    payload jsonb NOT NULL CHECK (jsonb_typeof(payload) = 'object'),
    captured_at timestamptz NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (commit_id, attempt_number),
    UNIQUE (evaluation_attempt_id)
);

CREATE FUNCTION guard_evaluation_policy_plan() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP = 'DELETE' THEN
        -- Parent cleanup may cascade; direct history deletion is forbidden.
        IF EXISTS (SELECT 1 FROM commits WHERE id = OLD.commit_id)
           AND EXISTS (SELECT 1 FROM evaluation_attempts WHERE id = OLD.evaluation_attempt_id) THEN
            RAISE EXCEPTION 'Evaluation policy plans are immutable';
        END IF;
        RETURN OLD;
    END IF;
    IF TG_OP = 'UPDATE' THEN
        RAISE EXCEPTION 'Evaluation policy plans are immutable';
    END IF;
    IF NOT EXISTS (
        SELECT 1 FROM evaluation_attempts a
        WHERE a.id = NEW.evaluation_attempt_id AND a.commit_id = NEW.commit_id
          AND a.attempt_number = NEW.attempt_number
    ) THEN
        RAISE EXCEPTION 'Evaluation policy plan attempt does not belong to commit';
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER evaluation_policy_plan_guard BEFORE INSERT OR UPDATE OR DELETE
    ON evaluation_policy_plans FOR EACH ROW EXECUTE FUNCTION guard_evaluation_policy_plan();

-- Inventory records discovery, not successful execution. Selected names retain
-- the build-scope intersection so excluded configurations never imply execution.
CREATE TABLE evaluation_attempt_inventory (
    evaluation_attempt_id uuid PRIMARY KEY,
    commit_id integer NOT NULL REFERENCES commits(id) ON DELETE CASCADE,
    attempt_number integer NOT NULL CHECK (attempt_number > 0),
    configuration_names text[] NOT NULL,
    selected_configuration_names text[] NOT NULL,
    execution_mode text NOT NULL CHECK (execution_mode IN ('native', 'mock')),
    captured_at timestamptz NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (evaluation_attempt_id, commit_id, attempt_number)
        REFERENCES evaluation_attempts(id, commit_id, attempt_number) ON DELETE CASCADE,
    CHECK (array_position(configuration_names, NULL) IS NULL),
    CHECK (array_position(selected_configuration_names, NULL) IS NULL),
    CHECK (selected_configuration_names <@ configuration_names)
);

CREATE INDEX evaluation_attempt_inventory_commit_attempt_idx
    ON evaluation_attempt_inventory(commit_id, attempt_number);

-- Match 0304: direct deletion is forbidden, but parent retention may cascade.
CREATE FUNCTION guard_evaluation_attempt_inventory() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP = 'DELETE' THEN
        IF NOT EXISTS (SELECT 1 FROM commits WHERE id = OLD.commit_id)
           OR NOT EXISTS (SELECT 1 FROM evaluation_attempts WHERE id = OLD.evaluation_attempt_id) THEN
            RETURN OLD;
        END IF;
    END IF;
    RAISE EXCEPTION 'Evaluation attempt inventory is first-write immutable';
END;
$$;

CREATE TRIGGER evaluation_attempt_inventory_guard BEFORE UPDATE OR DELETE
    ON evaluation_attempt_inventory FOR EACH ROW
    EXECUTE FUNCTION guard_evaluation_attempt_inventory();

COMMENT ON TABLE evaluation_attempt_inventory IS
    'First-write exact-attempt discovery inventory. Native names come from verified discovery; mock names are synthetic. Selection is not outcome or execution evidence. Names are sorted and deduplicated by the writer.';

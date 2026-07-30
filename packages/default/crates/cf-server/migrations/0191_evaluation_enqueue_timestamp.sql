-- Immutable commit-level enqueue time used for latest-per-flake evaluation ranking.

ALTER TABLE commits
    ADD COLUMN evaluation_enqueued_at timestamptz;

UPDATE commits
SET evaluation_enqueued_at = commit_timestamp
WHERE evaluation_enqueued_at IS NULL;

ALTER TABLE commits
    ALTER COLUMN evaluation_enqueued_at SET DEFAULT NOW(),
    ALTER COLUMN evaluation_enqueued_at SET NOT NULL;

CREATE FUNCTION prevent_evaluation_enqueued_at_update() RETURNS trigger AS $$
BEGIN
    IF NEW.evaluation_enqueued_at IS DISTINCT FROM OLD.evaluation_enqueued_at THEN
        RAISE EXCEPTION 'evaluation_enqueued_at is immutable';
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER prevent_evaluation_enqueued_at_update
    BEFORE UPDATE OF evaluation_enqueued_at ON commits
    FOR EACH ROW EXECUTE FUNCTION prevent_evaluation_enqueued_at_update();

CREATE INDEX commits_evaluation_latest_per_flake
    ON commits (flake_id, evaluation_enqueued_at DESC, id DESC);

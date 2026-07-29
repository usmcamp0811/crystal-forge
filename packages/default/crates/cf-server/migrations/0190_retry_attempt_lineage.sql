-- Durable, immutable build and evaluation retry attempts.

ALTER TABLE build_jobs
    ADD COLUMN parent_job_id uuid REFERENCES build_jobs(id) ON DELETE SET NULL,
    ADD COLUMN root_job_id uuid REFERENCES build_jobs(id) ON DELETE SET NULL,
    ADD COLUMN automatic_retry_source_id uuid REFERENCES build_jobs(id) ON DELETE SET NULL,
    ADD COLUMN attempt_number integer NOT NULL DEFAULT 1 CHECK (attempt_number >= 1),
    ADD COLUMN available_at timestamptz NOT NULL DEFAULT NOW();

UPDATE build_jobs SET root_job_id = id WHERE root_job_id IS NULL;

CREATE FUNCTION set_build_job_root_id() RETURNS trigger AS $$
BEGIN
    NEW.root_job_id := COALESCE(NEW.root_job_id, NEW.id);
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER set_build_job_root_id
    BEFORE INSERT ON build_jobs
    FOR EACH ROW EXECUTE FUNCTION set_build_job_root_id();

CREATE UNIQUE INDEX build_jobs_one_automatic_child_per_source
    ON build_jobs (automatic_retry_source_id)
    WHERE automatic_retry_source_id IS NOT NULL;

CREATE INDEX build_jobs_available_queue
    ON build_jobs (available_at, priority_weight DESC, created_at ASC)
    WHERE status = 'queued';

CREATE TABLE evaluation_attempts (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    commit_id integer NOT NULL REFERENCES commits(id) ON DELETE CASCADE,
    parent_attempt_id uuid REFERENCES evaluation_attempts(id) ON DELETE SET NULL,
    root_attempt_id uuid REFERENCES evaluation_attempts(id) ON DELETE SET NULL,
    automatic_retry_source_id uuid REFERENCES evaluation_attempts(id) ON DELETE SET NULL,
    attempt_number integer NOT NULL DEFAULT 1 CHECK (attempt_number >= 1),
    automatic_retry_count integer NOT NULL DEFAULT 0 CHECK (automatic_retry_count >= 0),
    status text NOT NULL DEFAULT 'queued'
        CHECK (status IN ('queued', 'in_progress', 'complete', 'failed', 'cancelled')),
    available_at timestamptz NOT NULL DEFAULT NOW(),
    started_at timestamptz,
    completed_at timestamptz,
    error_message text,
    failure_class text CHECK (failure_class IN ('transient', 'deterministic', 'cancelled', 'authorization', 'unknown')),
    created_at timestamptz NOT NULL DEFAULT NOW(),
    updated_at timestamptz NOT NULL DEFAULT NOW()
);

INSERT INTO evaluation_attempts (
    commit_id, status, attempt_number, automatic_retry_count, available_at,
    started_at, completed_at, error_message
)
SELECT
    id,
    CASE COALESCE(evaluation_status, 'pending')
        WHEN 'pending' THEN 'queued'
        WHEN 'in_progress' THEN 'in_progress'
        WHEN 'cancelling' THEN 'cancelled'
        WHEN 'complete' THEN 'complete'
        WHEN 'failed' THEN 'failed'
        WHEN 'cancelled' THEN 'cancelled'
    END,
    CASE
        WHEN COALESCE(evaluation_status, 'pending') = 'pending'
             AND COALESCE(evaluation_attempt_count, 0) > 0
            THEN evaluation_attempt_count + 1
        ELSE GREATEST(COALESCE(evaluation_attempt_count, 0), 1)
    END,
    CASE
        WHEN COALESCE(evaluation_status, 'pending') = 'pending'
            THEN GREATEST(COALESCE(evaluation_attempt_count, 0), 0)
        ELSE GREATEST(COALESCE(evaluation_attempt_count, 1) - 1, 0)
    END,
    COALESCE(evaluation_started_at, NOW()),
    evaluation_started_at,
    evaluation_completed_at,
    evaluation_error_message
FROM commits;

UPDATE evaluation_attempts SET root_attempt_id = id WHERE root_attempt_id IS NULL;

CREATE FUNCTION set_evaluation_attempt_root_id() RETURNS trigger AS $$
BEGIN
    NEW.root_attempt_id := COALESCE(NEW.root_attempt_id, NEW.id);
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER set_evaluation_attempt_root_id
    BEFORE INSERT ON evaluation_attempts
    FOR EACH ROW EXECUTE FUNCTION set_evaluation_attempt_root_id();

CREATE FUNCTION create_initial_evaluation_attempt() RETURNS trigger AS $$
BEGIN
    INSERT INTO evaluation_attempts (commit_id) VALUES (NEW.id);
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER create_initial_evaluation_attempt
    AFTER INSERT ON commits
    FOR EACH ROW EXECUTE FUNCTION create_initial_evaluation_attempt();

CREATE UNIQUE INDEX evaluation_attempts_one_automatic_child_per_source
    ON evaluation_attempts (automatic_retry_source_id)
    WHERE automatic_retry_source_id IS NOT NULL;

CREATE UNIQUE INDEX evaluation_attempts_one_active_per_commit
    ON evaluation_attempts (commit_id)
    WHERE status IN ('queued', 'in_progress');

CREATE INDEX evaluation_attempts_due_queue
    ON evaluation_attempts (available_at, created_at)
    WHERE status = 'queued';

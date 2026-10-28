-- TASK-440: durable targeted Config Inspector scheduling.
--
-- These rows describe enrichment work only. They do not select or retain
-- evaluation snapshots and never participate in build or deployment policy.

CREATE TABLE config_inspection_jobs (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    commit_id integer NOT NULL REFERENCES commits(id) ON DELETE CASCADE,
    derivation_id integer NOT NULL REFERENCES derivations(id) ON DELETE CASCADE,
    configuration_name text NOT NULL CHECK (btrim(configuration_name) <> ''),
    carrier_drv_path text NOT NULL CHECK (btrim(carrier_drv_path) <> ''),
    status text NOT NULL CHECK (status IN ('queued', 'running', 'succeeded', 'failed')),
    attempts integer NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    error text,
    scheduled_at timestamptz NOT NULL DEFAULT now(),
    started_at timestamptz,
    completed_at timestamptz,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    CHECK (
        (status = 'queued' AND started_at IS NULL AND completed_at IS NULL AND error IS NULL)
        OR (status = 'running' AND started_at IS NOT NULL AND completed_at IS NULL AND error IS NULL)
        OR (status = 'succeeded' AND started_at IS NOT NULL AND completed_at IS NOT NULL AND error IS NULL)
        OR (status = 'failed' AND started_at IS NOT NULL AND completed_at IS NOT NULL AND error IS NOT NULL)
    )
);

CREATE FUNCTION protect_config_inspection_job_target()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF NEW.commit_id IS DISTINCT FROM OLD.commit_id
       OR NEW.derivation_id IS DISTINCT FROM OLD.derivation_id
       OR NEW.configuration_name IS DISTINCT FROM OLD.configuration_name
       OR NEW.carrier_drv_path IS DISTINCT FROM OLD.carrier_drv_path THEN
        RAISE EXCEPTION 'Config inspection job target fields are immutable';
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER config_inspection_job_target_immutable
BEFORE UPDATE ON config_inspection_jobs
FOR EACH ROW EXECUTE FUNCTION protect_config_inspection_job_target();

CREATE UNIQUE INDEX config_inspection_jobs_active_target_idx
    ON config_inspection_jobs (commit_id, configuration_name)
    WHERE status IN ('queued', 'running');

CREATE INDEX config_inspection_jobs_queue_order_idx
    ON config_inspection_jobs (scheduled_at, created_at, id)
    WHERE status = 'queued';

CREATE INDEX config_inspection_jobs_derivation_idx
    ON config_inspection_jobs (derivation_id);

CREATE INDEX config_inspection_jobs_target_history_idx
    ON config_inspection_jobs (commit_id, configuration_name, created_at DESC);

COMMENT ON TABLE config_inspection_jobs IS
    'Durable targeted Config Inspector enrichment work; not evaluation, build, or deployment authority.';

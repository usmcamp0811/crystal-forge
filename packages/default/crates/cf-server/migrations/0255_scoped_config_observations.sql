-- TASK-440: immutable, scoped Config Explorer observations.
--
-- This subsystem is observational only. It does not select, retain, or modify
-- certified evaluation snapshots and cannot participate in deployment policy.

-- The leading derivation ID is already unique. This wider key cannot reject an
-- existing derivation row, but it lets dependent rows enforce the complete
-- commit, configuration, and carrier identity with a normal foreign key.
ALTER TABLE derivations
    ADD CONSTRAINT derivations_id_commit_name_path_unique
        UNIQUE (id, commit_id, derivation_name, derivation_path);

ALTER TABLE config_inspection_jobs
    DROP CONSTRAINT config_inspection_jobs_status_check,
    DROP CONSTRAINT config_inspection_jobs_check,
    DROP CONSTRAINT config_inspection_jobs_queued_execution_ck,
    ADD CONSTRAINT config_inspection_jobs_status_check
        CHECK (status IN ('queued', 'waiting_for_capacity', 'running', 'succeeded', 'failed')),
    ADD CONSTRAINT config_inspection_jobs_lifecycle_check CHECK (
        (status IN ('queued', 'waiting_for_capacity')
            AND started_at IS NULL AND completed_at IS NULL AND error IS NULL)
        OR (status = 'running'
            AND started_at IS NOT NULL AND completed_at IS NULL AND error IS NULL)
        OR (status = 'succeeded'
            AND started_at IS NOT NULL AND completed_at IS NOT NULL AND error IS NULL)
        OR (status = 'failed'
            AND started_at IS NOT NULL AND completed_at IS NOT NULL AND error IS NOT NULL)
    ),
    ADD CONSTRAINT config_inspection_jobs_queued_execution_ck CHECK (
        status NOT IN ('queued', 'waiting_for_capacity')
        OR (execution_id IS NULL AND execution_heartbeat_at IS NULL)
    ),
    -- COMPATIBILITY: Existing complete-inspection jobs can predate exact
    -- carrier fencing. Enforce exact identity for new writes without scanning
    -- or rewriting those historical rows during deployment.
    ADD CONSTRAINT config_inspection_jobs_derivation_identity_fk
        FOREIGN KEY (derivation_id, commit_id, configuration_name, carrier_drv_path)
        REFERENCES derivations (id, commit_id, derivation_name, derivation_path)
        ON DELETE CASCADE
        NOT VALID;

DROP INDEX config_inspection_jobs_active_target_idx;
CREATE UNIQUE INDEX config_inspection_jobs_active_target_idx
    ON config_inspection_jobs (commit_id, configuration_name)
    WHERE status IN ('queued', 'waiting_for_capacity', 'running');

DROP INDEX config_inspection_jobs_queue_order_idx;
CREATE INDEX config_inspection_jobs_queue_order_idx
    ON config_inspection_jobs (scheduled_at, created_at, id)
    WHERE status IN ('queued', 'waiting_for_capacity');

CREATE FUNCTION config_observation_path_valid(candidate jsonb, kind text)
RETURNS boolean LANGUAGE plpgsql IMMUTABLE AS $$
DECLARE
    component jsonb;
BEGIN
    IF candidate IS NULL OR jsonb_typeof(candidate) <> 'array'
       OR jsonb_array_length(candidate) > 16
       OR octet_length(candidate::text) > 8192
       OR ((kind IN ('root', 'configured_index')) <> (jsonb_array_length(candidate) = 0)) THEN
        RETURN false;
    END IF;
    FOR component IN SELECT value FROM jsonb_array_elements(candidate) LOOP
        IF jsonb_typeof(component) <> 'string'
           OR component #>> '{}' = ''
           OR char_length(component #>> '{}') > 256 THEN
            RETURN false;
        END IF;
    END LOOP;
    RETURN true;
EXCEPTION WHEN OTHERS THEN
    RETURN false;
END;
$$;

CREATE TABLE config_observation_contents (
    digest bytea PRIMARY KEY CHECK (octet_length(digest) = 32),
    schema_version integer NOT NULL CHECK (schema_version = 1),
    payload jsonb NOT NULL CHECK (jsonb_typeof(payload) = 'object'),
    content_bytes integer GENERATED ALWAYS AS (octet_length(payload::text)) STORED
        CHECK (content_bytes BETWEEN 2 AND 8388608),
    created_at timestamptz NOT NULL DEFAULT now(),
    CHECK (octet_length(payload::text) <= 8388608),
    UNIQUE (digest, schema_version)
);

CREATE TABLE config_observations (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    commit_id integer NOT NULL REFERENCES commits(id) ON DELETE CASCADE,
    derivation_id integer NOT NULL REFERENCES derivations(id) ON DELETE CASCADE,
    configuration_name text NOT NULL CHECK (btrim(configuration_name) <> ''),
    carrier_drv_path text NOT NULL CHECK (btrim(carrier_drv_path) <> ''),
    schema_version integer NOT NULL CHECK (schema_version = 1),
    path_components jsonb NOT NULL,
    kind text NOT NULL CHECK (kind IN ('root', 'prefix', 'option', 'provenance', 'configured_index')),
    content_digest bytea NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    CHECK (config_observation_path_valid(path_components, kind)),
    FOREIGN KEY (derivation_id, commit_id, configuration_name, carrier_drv_path)
        REFERENCES derivations (id, commit_id, derivation_name, derivation_path)
        ON DELETE CASCADE,
    FOREIGN KEY (content_digest, schema_version)
        REFERENCES config_observation_contents(digest, schema_version),
    UNIQUE (
        commit_id, configuration_name, derivation_id, carrier_drv_path,
        schema_version, path_components, kind
    ),
    UNIQUE (
        id, commit_id, configuration_name, derivation_id, carrier_drv_path,
        schema_version, path_components, kind
    )
);

CREATE TABLE config_observation_requests (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    commit_id integer NOT NULL REFERENCES commits(id) ON DELETE CASCADE,
    derivation_id integer NOT NULL REFERENCES derivations(id) ON DELETE CASCADE,
    configuration_name text NOT NULL CHECK (btrim(configuration_name) <> ''),
    carrier_drv_path text NOT NULL CHECK (btrim(carrier_drv_path) <> ''),
    schema_version integer NOT NULL CHECK (schema_version = 1),
    path_components jsonb NOT NULL,
    kind text NOT NULL CHECK (kind IN ('root', 'prefix', 'option', 'provenance', 'configured_index')),
    priority smallint NOT NULL CHECK (priority IN (10, 100)),
    status text NOT NULL CHECK (status IN ('queued', 'waiting_for_capacity', 'running', 'succeeded', 'failed')),
    attempts integer NOT NULL DEFAULT 0 CHECK (attempts BETWEEN 0 AND 3),
    execution_id uuid,
    execution_heartbeat_at timestamptz,
    observation_id uuid,
    error text CHECK (error IS NULL OR char_length(error) BETWEEN 1 AND 4096),
    scheduled_at timestamptz NOT NULL DEFAULT now(),
    started_at timestamptz,
    completed_at timestamptz,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    CHECK (config_observation_path_valid(path_components, kind)),
    FOREIGN KEY (derivation_id, commit_id, configuration_name, carrier_drv_path)
        REFERENCES derivations (id, commit_id, derivation_name, derivation_path)
        ON DELETE CASCADE,
    FOREIGN KEY (
        observation_id, commit_id, configuration_name, derivation_id,
        carrier_drv_path, schema_version, path_components, kind
    ) REFERENCES config_observations (
        id, commit_id, configuration_name, derivation_id,
        carrier_drv_path, schema_version, path_components, kind
    ) ON DELETE CASCADE,
    CHECK ((kind = 'configured_index' AND priority = 100) OR (kind <> 'configured_index' AND priority = 10)),
    CHECK (
        (status IN ('queued', 'waiting_for_capacity') AND attempts = 0
            AND execution_id IS NULL AND execution_heartbeat_at IS NULL
            AND observation_id IS NULL AND error IS NULL
            AND started_at IS NULL AND completed_at IS NULL)
        OR (status = 'running' AND attempts BETWEEN 1 AND 3
            AND execution_id IS NOT NULL AND execution_heartbeat_at IS NOT NULL
            AND observation_id IS NULL AND error IS NULL
            AND started_at IS NOT NULL AND completed_at IS NULL)
        OR (status = 'succeeded' AND attempts BETWEEN 0 AND 3
            AND observation_id IS NOT NULL AND error IS NULL
            AND completed_at IS NOT NULL)
        OR (status = 'failed' AND attempts BETWEEN 1 AND 3
            AND execution_id IS NOT NULL AND execution_heartbeat_at IS NOT NULL
            AND observation_id IS NULL AND error IS NOT NULL
            AND started_at IS NOT NULL AND completed_at IS NOT NULL)
    )
);

CREATE UNIQUE INDEX config_observation_requests_active_identity_idx
    ON config_observation_requests (
        commit_id, configuration_name, derivation_id, carrier_drv_path,
        schema_version, path_components, kind
    )
    WHERE status IN ('queued', 'waiting_for_capacity', 'running');

CREATE INDEX config_observation_requests_queue_idx
    ON config_observation_requests (priority, scheduled_at, created_at, id)
    WHERE status IN ('queued', 'waiting_for_capacity');

CREATE INDEX config_observation_requests_execution_idx
    ON config_observation_requests (execution_id)
    WHERE execution_id IS NOT NULL;

CREATE FUNCTION protect_config_observation_immutable_fields()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF TG_TABLE_NAME = 'config_observation_contents' THEN
        RAISE EXCEPTION 'Config observation contents are immutable';
    END IF;
    IF NEW.commit_id IS DISTINCT FROM OLD.commit_id
       OR NEW.derivation_id IS DISTINCT FROM OLD.derivation_id
       OR NEW.configuration_name IS DISTINCT FROM OLD.configuration_name
       OR NEW.carrier_drv_path IS DISTINCT FROM OLD.carrier_drv_path
       OR NEW.schema_version IS DISTINCT FROM OLD.schema_version
       OR NEW.path_components IS DISTINCT FROM OLD.path_components
       OR NEW.kind IS DISTINCT FROM OLD.kind THEN
        RAISE EXCEPTION 'Config observation identity fields are immutable';
    END IF;
    IF TG_TABLE_NAME = 'config_observations' THEN
        RAISE EXCEPTION 'Config observations are immutable';
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER config_observation_contents_immutable
BEFORE UPDATE ON config_observation_contents
FOR EACH ROW EXECUTE FUNCTION protect_config_observation_immutable_fields();

CREATE TRIGGER config_observations_immutable
BEFORE UPDATE ON config_observations
FOR EACH ROW EXECUTE FUNCTION protect_config_observation_immutable_fields();

CREATE TRIGGER config_observation_request_identity_immutable
BEFORE UPDATE ON config_observation_requests
FOR EACH ROW EXECUTE FUNCTION protect_config_observation_immutable_fields();

COMMENT ON TABLE config_observation_requests IS
    'Non-authoritative scoped Config Explorer work with truthful capacity and execution lifecycle.';
COMMENT ON TABLE config_observations IS
    'Immutable scoped observations separate from certified evaluation snapshots and selectors.';

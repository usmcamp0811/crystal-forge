-- TASK-440: make immediate-child page offsets part of scoped observation identity.

DO $$
DECLARE
    constraint_name text;
BEGIN
    SELECT conname INTO STRICT constraint_name
    FROM pg_constraint
    WHERE conrelid = 'config_observation_requests'::regclass
      AND confrelid = 'config_observations'::regclass
      AND contype = 'f';
    EXECUTE format(
        'ALTER TABLE config_observation_requests DROP CONSTRAINT %I',
        constraint_name
    );

    FOR constraint_name IN
        SELECT conname
        FROM pg_constraint
        WHERE conrelid = 'config_observations'::regclass
          AND contype = 'u'
    LOOP
        EXECUTE format(
            'ALTER TABLE config_observations DROP CONSTRAINT %I',
            constraint_name
        );
    END LOOP;
END;
$$;

DROP INDEX config_observation_requests_active_identity_idx;

ALTER TABLE config_observations
    ADD COLUMN child_offset integer NOT NULL DEFAULT 0,
    ADD CONSTRAINT config_observations_child_offset_check CHECK (
        child_offset BETWEEN 0 AND 1000000
        AND (kind IN ('root', 'prefix') OR child_offset = 0)
    ),
    ADD CONSTRAINT config_observations_identity_unique UNIQUE (
        commit_id, configuration_name, derivation_id, carrier_drv_path,
        schema_version, path_components, kind, child_offset
    ),
    ADD CONSTRAINT config_observations_fk_identity_unique UNIQUE (
        id, commit_id, configuration_name, derivation_id, carrier_drv_path,
        schema_version, path_components, kind, child_offset
    );

ALTER TABLE config_observation_requests
    ADD COLUMN child_offset integer NOT NULL DEFAULT 0,
    ADD CONSTRAINT config_observation_requests_child_offset_check CHECK (
        child_offset BETWEEN 0 AND 1000000
        AND (kind IN ('root', 'prefix') OR child_offset = 0)
    ),
    ADD CONSTRAINT config_observation_requests_observation_identity_fk FOREIGN KEY (
        observation_id, commit_id, configuration_name, derivation_id,
        carrier_drv_path, schema_version, path_components, kind, child_offset
    ) REFERENCES config_observations (
        id, commit_id, configuration_name, derivation_id,
        carrier_drv_path, schema_version, path_components, kind, child_offset
    ) ON DELETE CASCADE;

CREATE UNIQUE INDEX config_observation_requests_active_identity_idx
    ON config_observation_requests (
        commit_id, configuration_name, derivation_id, carrier_drv_path,
        schema_version, path_components, kind, child_offset
    )
    WHERE status IN ('queued', 'waiting_for_capacity', 'running');

CREATE OR REPLACE FUNCTION protect_config_observation_immutable_fields()
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
       OR NEW.kind IS DISTINCT FROM OLD.kind
       OR NEW.child_offset IS DISTINCT FROM OLD.child_offset THEN
        RAISE EXCEPTION 'Config observation identity fields are immutable';
    END IF;
    IF TG_TABLE_NAME = 'config_observations' THEN
        RAISE EXCEPTION 'Config observations are immutable';
    END IF;
    RETURN NEW;
END;
$$;

COMMENT ON COLUMN config_observations.child_offset IS
    'Server-validated immediate-child offset. Only root and prefix observations can use a nonzero value.';
COMMENT ON COLUMN config_observation_requests.child_offset IS
    'Server-validated immediate-child offset included in cache, active-request, and observation FK identity.';

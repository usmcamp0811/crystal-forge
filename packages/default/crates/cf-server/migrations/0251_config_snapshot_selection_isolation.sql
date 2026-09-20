-- TASK-440: isolate targeted Config Inspector selection from primary evaluation.
--
-- evaluation_snapshot_selections remains the current primary evaluation-attempt
-- selector and accepts only schema-V1 artifacts. Targeted Config Inspector
-- persistence uses the independent schema-V2 selector below. Both selectors
-- reference the same immutable evaluation_snapshots table.

CREATE TABLE config_snapshot_selections (
    commit_id integer NOT NULL REFERENCES commits(id) ON DELETE CASCADE,
    configuration_name text NOT NULL CHECK (btrim(configuration_name) <> ''),
    current_snapshot_id uuid NOT NULL,
    updated_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (commit_id, configuration_name),
    FOREIGN KEY (current_snapshot_id, commit_id, configuration_name)
        REFERENCES evaluation_snapshots (id, commit_id, configuration_name)
        ON DELETE RESTRICT DEFERRABLE INITIALLY DEFERRED
);

CREATE INDEX config_snapshot_selections_snapshot_idx
    ON config_snapshot_selections(current_snapshot_id);

-- The intermediate V2 writer advanced the primary selector. Backfill the
-- targeted selector from the latest immutable V2 attempt, including unavailable
-- attempts because the selector represents the latest attempt, not only success.
INSERT INTO config_snapshot_selections (
    commit_id, configuration_name, current_snapshot_id, updated_at
)
SELECT DISTINCT ON (snapshot.commit_id, snapshot.configuration_name)
       snapshot.commit_id,
       snapshot.configuration_name,
       snapshot.id,
       COALESCE(snapshot.completed_at, snapshot.created_at)
FROM evaluation_snapshots snapshot
WHERE snapshot.schema_version = 2
ORDER BY snapshot.commit_id, snapshot.configuration_name,
         snapshot.created_at DESC, snapshot.id DESC;

-- Repair only primary selector rows contaminated by the intermediate V2 writer.
-- Existing valid V1 selector identities remain unchanged.
UPDATE evaluation_snapshot_selections selection
SET current_snapshot_id = latest.id,
    updated_at = now()
FROM evaluation_snapshots current_snapshot
JOIN LATERAL (
    SELECT snapshot.id
    FROM evaluation_snapshots snapshot
    WHERE snapshot.commit_id = current_snapshot.commit_id
      AND snapshot.configuration_name = current_snapshot.configuration_name
      AND snapshot.schema_version = 1
    ORDER BY snapshot.created_at DESC, snapshot.id DESC
    LIMIT 1
) latest ON true
WHERE selection.current_snapshot_id = current_snapshot.id
  AND current_snapshot.schema_version = 2;

-- No V1 attempt exists for this key, so remove the contaminated primary row
-- instead of pointing primary evaluation authority at a V2 artifact.
DELETE FROM evaluation_snapshot_selections selection
USING evaluation_snapshots current_snapshot
WHERE selection.current_snapshot_id = current_snapshot.id
  AND current_snapshot.schema_version = 2
  AND NOT EXISTS (
      SELECT 1
      FROM evaluation_snapshots v1
      WHERE v1.commit_id = current_snapshot.commit_id
        AND v1.configuration_name = current_snapshot.configuration_name
        AND v1.schema_version = 1
  );

CREATE FUNCTION validate_primary_evaluation_snapshot_selection_v1()
RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE
    target_commit_id integer;
    target_configuration_name text;
    target_schema_version integer;
BEGIN
    SELECT snapshot.commit_id, snapshot.configuration_name, snapshot.schema_version
    INTO target_commit_id, target_configuration_name, target_schema_version
    FROM evaluation_snapshots snapshot
    WHERE snapshot.id = NEW.current_snapshot_id;

    IF NOT FOUND THEN
        RAISE EXCEPTION
            'primary evaluation selector target % does not exist',
            NEW.current_snapshot_id;
    END IF;
    IF target_commit_id <> NEW.commit_id
       OR target_configuration_name <> NEW.configuration_name THEN
        RAISE EXCEPTION
            'primary evaluation selector target % does not match commit/configuration',
            NEW.current_snapshot_id;
    END IF;
    IF target_schema_version <> 1 THEN
        RAISE EXCEPTION
            'primary evaluation selector requires schema-V1 target, got schema version %',
            target_schema_version;
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER evaluation_snapshot_selections_v1_only
BEFORE INSERT OR UPDATE OF current_snapshot_id, commit_id, configuration_name
ON evaluation_snapshot_selections
FOR EACH ROW EXECUTE FUNCTION validate_primary_evaluation_snapshot_selection_v1();

CREATE FUNCTION validate_config_snapshot_selection_v2()
RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE
    target_commit_id integer;
    target_configuration_name text;
    target_schema_version integer;
BEGIN
    SELECT snapshot.commit_id, snapshot.configuration_name, snapshot.schema_version
    INTO target_commit_id, target_configuration_name, target_schema_version
    FROM evaluation_snapshots snapshot
    WHERE snapshot.id = NEW.current_snapshot_id;

    IF NOT FOUND THEN
        RAISE EXCEPTION
            'Config selector target % does not exist',
            NEW.current_snapshot_id;
    END IF;
    IF target_commit_id <> NEW.commit_id
       OR target_configuration_name <> NEW.configuration_name THEN
        RAISE EXCEPTION
            'Config selector target % does not match commit/configuration',
            NEW.current_snapshot_id;
    END IF;
    IF target_schema_version <> 2 THEN
        RAISE EXCEPTION
            'Config selector requires schema-V2 target, got schema version %',
            target_schema_version;
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER config_snapshot_selections_v2_only
BEFORE INSERT OR UPDATE OF current_snapshot_id, commit_id, configuration_name
ON config_snapshot_selections
FOR EACH ROW EXECUTE FUNCTION validate_config_snapshot_selection_v2();

COMMENT ON TABLE evaluation_snapshot_selections IS
    'Current PRIMARY evaluation-attempt selector. Schema V1 only. Deployment, enforcement, and host-delta authority use this pointer; targeted Config inspection must not update it.';
COMMENT ON TABLE config_snapshot_selections IS
    'Current TARGETED Config Inspector attempt selector. Schema V2 only and independent from deployment and primary-evaluation authority.';

DO $$
BEGIN
    IF EXISTS (
        SELECT 1
        FROM evaluation_snapshot_selections selection
        JOIN evaluation_snapshots snapshot ON snapshot.id = selection.current_snapshot_id
        WHERE snapshot.schema_version <> 1
    ) THEN
        RAISE EXCEPTION 'primary evaluation selector contains a non-V1 artifact';
    END IF;
    IF EXISTS (
        SELECT 1
        FROM config_snapshot_selections selection
        JOIN evaluation_snapshots snapshot ON snapshot.id = selection.current_snapshot_id
        WHERE snapshot.schema_version <> 2
    ) THEN
        RAISE EXCEPTION 'Config selector contains a non-V2 artifact';
    END IF;
END;
$$;

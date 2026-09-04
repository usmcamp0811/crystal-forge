-- TASK-440 review remediation: immutable evaluation artifacts and current selectors.
--
-- Existing evaluation_snapshots rows become version 1 artifacts. A selector
-- identifies the current attempt for each commit and configuration. Retained
-- generations continue to reference a successful artifact after the selector
-- advances. Pre-0248 rows remain queryable, but their deployment/store lineage
-- is not marked verified because the mutable schema did not preserve that fact.

DROP TRIGGER evaluation_snapshots_bump_version ON evaluation_snapshots;
DROP FUNCTION bump_evaluation_snapshot_version();

ALTER TABLE evaluation_snapshots
    DROP CONSTRAINT evaluation_snapshots_commit_id_configuration_name_key;

ALTER TABLE evaluation_snapshots
    ADD COLUMN integrity_version smallint NOT NULL DEFAULT 0
        CHECK (integrity_version IN (0, 1)),
    ADD CONSTRAINT evaluation_snapshots_id_commit_configuration_unique
        UNIQUE (id, commit_id, configuration_name);

CREATE TABLE evaluation_snapshot_selections (
    commit_id integer NOT NULL REFERENCES commits(id) ON DELETE CASCADE,
    configuration_name text NOT NULL CHECK (btrim(configuration_name) <> ''),
    current_snapshot_id uuid NOT NULL,
    updated_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (commit_id, configuration_name),
    FOREIGN KEY (current_snapshot_id, commit_id, configuration_name)
        REFERENCES evaluation_snapshots (id, commit_id, configuration_name)
        ON DELETE RESTRICT DEFERRABLE INITIALLY DEFERRED
);

INSERT INTO evaluation_snapshot_selections (
    commit_id, configuration_name, current_snapshot_id, updated_at
)
SELECT commit_id, configuration_name, id, COALESCE(completed_at, created_at)
FROM evaluation_snapshots;

ALTER TABLE pending_system_deployments
    ADD COLUMN evaluation_snapshot_id uuid,
    ADD COLUMN requested_derivation_id integer,
    ADD COLUMN evaluation_snapshot_binding_expected boolean NOT NULL DEFAULT false;

-- A pre-0248 retry updated the one snapshot row in place. Failed retries left
-- the prior option references present but changed the lifecycle and counters.
-- A retained reference proves that the row was available when it was retained.
-- Recover that successful artifact before the current failed row becomes
-- immutable, and leave the current selector on the failed retry.
CREATE TEMP TABLE recovered_retained_evaluation_artifacts (
    overwritten_snapshot_id uuid PRIMARY KEY,
    recovered_snapshot_id uuid NOT NULL UNIQUE DEFAULT gen_random_uuid()
) ON COMMIT DROP;

INSERT INTO recovered_retained_evaluation_artifacts (overwritten_snapshot_id)
SELECT DISTINCT snapshot.id
FROM evaluation_generation_snapshots retained
JOIN evaluation_snapshots snapshot ON snapshot.id = retained.snapshot_id
WHERE snapshot.lifecycle <> 'available';

INSERT INTO evaluation_snapshots (
    id, commit_id, configuration_name, schema_version, lifecycle,
    first_parent_sha, error, option_count, module_count,
    evaluation_duration_ms, content_bytes, created_at, completed_at,
    host_delta_count, snapshot_version
)
SELECT recovered.recovered_snapshot_id,
       overwritten.commit_id,
       overwritten.configuration_name,
       overwritten.schema_version,
       'available',
       overwritten.first_parent_sha,
       NULL,
       (SELECT COUNT(*)::integer
        FROM evaluation_snapshot_options item
        WHERE item.snapshot_id = overwritten.id),
       (SELECT COUNT(DISTINCT (
            definition.value->>'source_input',
            definition.value->>'source_revision',
            definition.value->>'source_path'
        ))::integer
        FROM evaluation_snapshot_options item
        JOIN evaluation_option_contents content ON content.digest = item.content_digest
        CROSS JOIN LATERAL jsonb_array_elements(
            CASE WHEN jsonb_typeof(content.payload->'definitions') = 'array'
                 THEN content.payload->'definitions' ELSE '[]'::jsonb END
        ) definition(value)
        WHERE item.snapshot_id = overwritten.id),
       NULL,
       (SELECT COALESCE(SUM(octet_length(content.payload::text)), 0)::bigint
        FROM evaluation_snapshot_options item
        JOIN evaluation_option_contents content ON content.digest = item.content_digest
        WHERE item.snapshot_id = overwritten.id),
       overwritten.created_at,
       COALESCE((
           SELECT MIN(COALESCE(derivation.completed_at, retained.retained_at))
           FROM evaluation_generation_snapshots retained
           JOIN derivations derivation ON derivation.id = retained.derivation_id
           WHERE retained.snapshot_id = overwritten.id
       ), overwritten.completed_at),
       overwritten.host_delta_count,
       overwritten.snapshot_version
FROM recovered_retained_evaluation_artifacts recovered
JOIN evaluation_snapshots overwritten
  ON overwritten.id = recovered.overwritten_snapshot_id;

INSERT INTO evaluation_snapshot_options (
    snapshot_id, option_path, content_digest, is_overridden
)
SELECT recovered.recovered_snapshot_id,
       item.option_path, item.content_digest, item.is_overridden
FROM recovered_retained_evaluation_artifacts recovered
JOIN evaluation_snapshot_options item
  ON item.snapshot_id = recovered.overwritten_snapshot_id;

UPDATE evaluation_generation_snapshots retained
SET snapshot_id = recovered.recovered_snapshot_id
FROM recovered_retained_evaluation_artifacts recovered
WHERE retained.snapshot_id = recovered.overwritten_snapshot_id;

ALTER TABLE evaluation_generation_snapshots
    ADD COLUMN configuration_name text,
    ADD COLUMN lineage_verified boolean NOT NULL DEFAULT false;

UPDATE evaluation_generation_snapshots retained
SET configuration_name = snapshot.configuration_name
FROM evaluation_snapshots snapshot
WHERE snapshot.id = retained.snapshot_id;

ALTER TABLE evaluation_generation_snapshots
    ALTER COLUMN configuration_name SET NOT NULL,
    ADD CONSTRAINT evaluation_generation_snapshot_artifact_lineage_fk
        FOREIGN KEY (snapshot_id, commit_id, configuration_name)
        REFERENCES evaluation_snapshots (id, commit_id, configuration_name)
        ON DELETE RESTRICT DEFERRABLE INITIALLY DEFERRED;

-- COMPATIBILITY: The mutable pre-0248 schema cannot prove whether a current
-- available row existed when a historical deployment was issued. Leave legacy
-- deployments unbound. New evaluation finalization binds only an exact current
-- deployment, and ambiguous historical rows fail closed for retention.

ALTER TABLE pending_system_deployments
    ADD CONSTRAINT pending_system_deployments_evaluation_snapshot_fk
        FOREIGN KEY (evaluation_snapshot_id, requested_commit_id)
        REFERENCES evaluation_snapshots (id, commit_id)
        ON DELETE RESTRICT DEFERRABLE INITIALLY DEFERRED;

ALTER TABLE pending_system_deployments
    ADD CONSTRAINT pending_system_deployments_requested_derivation_fk
        FOREIGN KEY (requested_derivation_id, requested_commit_id)
        REFERENCES derivations (id, commit_id)
        ON DELETE RESTRICT DEFERRABLE INITIALLY DEFERRED;

ALTER TABLE derivations
    ADD CONSTRAINT derivations_id_commit_name_unique
        UNIQUE (id, commit_id, derivation_name);

ALTER TABLE evaluation_generation_snapshots
    ADD CONSTRAINT evaluation_generation_snapshot_derivation_lineage_fk
        FOREIGN KEY (derivation_id, commit_id, configuration_name)
        REFERENCES derivations (id, commit_id, derivation_name)
        ON DELETE RESTRICT DEFERRABLE INITIALLY DEFERRED;

-- These validators mirror the Serde shapes of SafeOptionValue and
-- OptionDefinitionProvenance. They run once for legacy backfill and once before
-- a new artifact receives its immutable integrity marker. Bounded API reads can
-- then check one scalar without scanning or transferring the complete corpus.
CREATE FUNCTION evaluation_safe_option_value_valid(candidate jsonb)
RETURNS boolean LANGUAGE plpgsql IMMUTABLE STRICT AS $$
DECLARE
    kind text;
    element jsonb;
BEGIN
    IF jsonb_typeof(candidate) <> 'object'
       OR jsonb_typeof(candidate->'kind') <> 'string'
       OR NOT candidate ? 'value' THEN
        RETURN false;
    END IF;
    kind := candidate->>'kind';
    IF kind = 'scalar' THEN
        RETURN jsonb_typeof(candidate->'value') IN (
            'string', 'number', 'boolean', 'null'
        );
    ELSIF kind = 'package' THEN
        IF jsonb_typeof(candidate->'value') <> 'object' THEN
            RETURN false;
        END IF;
        RETURN NOT EXISTS (
            SELECT 1
            FROM unnest(ARRAY['name', 'pname', 'version', 'output_path']) field
            WHERE candidate->'value' ? field
              AND jsonb_typeof(candidate->'value'->field) NOT IN ('string', 'null')
        );
    ELSIF kind = 'list' THEN
        IF jsonb_typeof(candidate->'value') <> 'array' THEN
            RETURN false;
        END IF;
        FOR element IN SELECT value FROM jsonb_array_elements(candidate->'value') LOOP
            IF NOT evaluation_safe_option_value_valid(element) THEN
                RETURN false;
            END IF;
        END LOOP;
        RETURN true;
    ELSIF kind IN ('attribute_set', 'submodule') THEN
        RETURN COALESCE(jsonb_typeof(candidate->'value') = 'object', false);
    ELSIF kind = 'opaque' THEN
        RETURN COALESCE(
            jsonb_typeof(candidate->'value') = 'object'
            AND jsonb_typeof(candidate->'value'->'type_name') = 'string',
            false
        );
    ELSIF kind = 'failed' THEN
        RETURN COALESCE(
            jsonb_typeof(candidate->'value') = 'object'
            AND jsonb_typeof(candidate->'value'->'code') = 'string'
            AND jsonb_typeof(candidate->'value'->'message') = 'string',
            false
        );
    END IF;
    RETURN false;
EXCEPTION WHEN OTHERS THEN
    RETURN false;
END;
$$;

CREATE FUNCTION evaluation_option_provenance_valid(candidate jsonb)
RETURNS boolean LANGUAGE plpgsql IMMUTABLE STRICT AS $$
BEGIN
    RETURN COALESCE(jsonb_typeof(candidate) = 'object'
       AND jsonb_typeof(candidate->'source_path') = 'string'
       AND jsonb_typeof(candidate->'winning') = 'boolean'
       AND (NOT candidate ? 'source_input'
            OR jsonb_typeof(candidate->'source_input') IN ('string', 'null'))
       AND (NOT candidate ? 'source_revision'
            OR jsonb_typeof(candidate->'source_revision') IN ('string', 'null'))
       AND (NOT candidate ? 'priority'
            OR jsonb_typeof(candidate->'priority') = 'null'
            OR (jsonb_typeof(candidate->'priority') = 'number'
                AND (candidate->>'priority')::bigint::text = candidate->>'priority'))
       AND (NOT candidate ? 'status'
            OR jsonb_typeof(candidate->'status') IN ('string', 'null'))
       AND (NOT candidate ? 'winner_note'
            OR jsonb_typeof(candidate->'winner_note') IN ('string', 'null'))
       AND (NOT candidate ? 'tracked_flake'
            OR jsonb_typeof(candidate->'tracked_flake') = 'null'), false);
EXCEPTION WHEN OTHERS THEN
    RETURN false;
END;
$$;

CREATE FUNCTION evaluation_option_payload_valid(candidate jsonb)
RETURNS boolean LANGUAGE plpgsql IMMUTABLE STRICT AS $$
BEGIN
    RETURN COALESCE(jsonb_typeof(candidate) = 'object'
       AND jsonb_typeof(candidate->'declared_type') = 'string'
       AND jsonb_typeof(candidate->'overridden') = 'boolean'
       AND jsonb_typeof(candidate->'definitions') = 'array'
       AND evaluation_safe_option_value_valid(candidate->'value')
       AND NOT EXISTS (
           SELECT 1
           FROM jsonb_array_elements(candidate->'definitions') definition(value)
           WHERE NOT evaluation_option_provenance_valid(definition.value)
       ), false);
EXCEPTION WHEN OTHERS THEN
    RETURN false;
END;
$$;

CREATE FUNCTION evaluation_snapshot_payloads_valid(target_snapshot_id uuid)
RETURNS boolean LANGUAGE sql STABLE STRICT AS $$
    SELECT EXISTS (
        SELECT 1
        FROM evaluation_snapshots snapshot
        WHERE snapshot.id = target_snapshot_id
          AND snapshot.lifecycle = 'available'
          AND snapshot.schema_version = 1
          AND snapshot.option_count = (
              SELECT COUNT(*) FROM evaluation_snapshot_options item
              WHERE item.snapshot_id = snapshot.id
          )
          AND snapshot.module_count = (
              SELECT COUNT(DISTINCT (
                  definition.value->>'source_input',
                  definition.value->>'source_revision',
                  definition.value->>'source_path'
              ))
              FROM evaluation_snapshot_options item
              JOIN evaluation_option_contents content
                ON content.digest = item.content_digest
              CROSS JOIN LATERAL jsonb_array_elements(
                  CASE WHEN jsonb_typeof(content.payload->'definitions') = 'array'
                       THEN content.payload->'definitions' ELSE '[]'::jsonb END
              ) definition(value)
              WHERE item.snapshot_id = snapshot.id
          )
          AND NOT EXISTS (
              SELECT 1
              FROM evaluation_snapshot_options item
              LEFT JOIN evaluation_option_contents content
                ON content.digest = item.content_digest
              WHERE item.snapshot_id = snapshot.id
                AND (content.digest IS NULL OR content.schema_version <> 1
                  OR NOT evaluation_option_payload_valid(content.payload)
                  OR content.payload->'overridden' IS DISTINCT FROM
                     to_jsonb(item.is_overridden))
          )
    )
$$;

UPDATE evaluation_snapshots snapshot
SET integrity_version = 1
WHERE snapshot.lifecycle = 'available'
  AND evaluation_snapshot_payloads_valid(snapshot.id);

CREATE FUNCTION preserve_evaluation_snapshot_artifact()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    -- Certification is a post-insert transition. At that point the validator
    -- can inspect every option reference and shared content row in this
    -- transaction. An INSERT cannot bypass complete validation.
    IF TG_OP = 'INSERT' THEN
        IF NEW.integrity_version <> 0 THEN
            RAISE EXCEPTION
                'evaluation snapshot integrity must be certified after insertion';
        END IF;
        RETURN NEW;
    END IF;
    -- host_delta_count is derived from the complete current same-commit corpus.
    -- It is not part of the evaluator artifact identity and can be recomputed.
    IF NEW.id IS DISTINCT FROM OLD.id
       OR NEW.commit_id IS DISTINCT FROM OLD.commit_id
       OR NEW.configuration_name IS DISTINCT FROM OLD.configuration_name
       OR NEW.schema_version IS DISTINCT FROM OLD.schema_version
       OR NEW.lifecycle IS DISTINCT FROM OLD.lifecycle
       OR NEW.first_parent_sha IS DISTINCT FROM OLD.first_parent_sha
       OR NEW.error IS DISTINCT FROM OLD.error
       OR NEW.option_count IS DISTINCT FROM OLD.option_count
       OR NEW.module_count IS DISTINCT FROM OLD.module_count
       OR NEW.evaluation_duration_ms IS DISTINCT FROM OLD.evaluation_duration_ms
       OR NEW.content_bytes IS DISTINCT FROM OLD.content_bytes
       OR NEW.created_at IS DISTINCT FROM OLD.created_at
       OR NEW.completed_at IS DISTINCT FROM OLD.completed_at
       OR NEW.snapshot_version IS DISTINCT FROM OLD.snapshot_version
       OR (NEW.integrity_version IS DISTINCT FROM OLD.integrity_version AND NOT (
           OLD.integrity_version = 0
           AND NEW.integrity_version = 1
           AND evaluation_snapshot_payloads_valid(NEW.id)
       )) THEN
        RAISE EXCEPTION 'evaluation snapshot artifacts are immutable';
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER evaluation_snapshot_artifact_immutable
BEFORE INSERT OR UPDATE ON evaluation_snapshots
FOR EACH ROW EXECUTE FUNCTION preserve_evaluation_snapshot_artifact();

CREATE FUNCTION preserve_evaluation_option_content()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    RAISE EXCEPTION 'evaluation option content is immutable';
END;
$$;

CREATE TRIGGER evaluation_option_content_immutable
BEFORE UPDATE ON evaluation_option_contents
FOR EACH ROW EXECUTE FUNCTION preserve_evaluation_option_content();

CREATE FUNCTION preserve_evaluation_snapshot_option()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP = 'INSERT' AND EXISTS (
        SELECT 1 FROM evaluation_snapshots snapshot
        WHERE snapshot.id = NEW.snapshot_id AND snapshot.integrity_version = 1
    ) THEN
        RAISE EXCEPTION 'certified evaluation snapshot option references are immutable';
    END IF;
    IF TG_OP = 'UPDATE' THEN
        RAISE EXCEPTION 'evaluation snapshot option references are immutable';
    END IF;
    -- PostgreSQL executes this trigger recursively for an ON DELETE CASCADE
    -- from its owning artifact. Direct reference deletion is not permitted.
    IF TG_OP = 'DELETE' AND pg_trigger_depth() = 1 THEN
        RAISE EXCEPTION 'evaluation snapshot option references are immutable';
    END IF;
    RETURN CASE WHEN TG_OP = 'INSERT' THEN NEW ELSE OLD END;
END;
$$;

CREATE TRIGGER evaluation_snapshot_option_immutable
BEFORE INSERT OR UPDATE OR DELETE ON evaluation_snapshot_options
FOR EACH ROW EXECUTE FUNCTION preserve_evaluation_snapshot_option();

CREATE FUNCTION enforce_retained_evaluation_artifact()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP = 'UPDATE' THEN
        RAISE EXCEPTION 'retained evaluation artifacts are immutable';
    END IF;
    IF NOT NEW.lineage_verified THEN
        RAISE EXCEPTION
            'new retained generations require verified deployment lineage';
    END IF;
    IF NOT EXISTS (
        SELECT 1
        FROM evaluation_snapshots snapshot
        WHERE snapshot.id = NEW.snapshot_id
          AND snapshot.commit_id = NEW.commit_id
          AND snapshot.configuration_name = NEW.configuration_name
          AND snapshot.lifecycle = 'available'
          AND snapshot.integrity_version = 1
    ) THEN
        RAISE EXCEPTION
            'retained generation requires an exact successful evaluation artifact';
    END IF;
    IF NOT EXISTS (
        SELECT 1
        FROM derivations derivation
        WHERE derivation.id = NEW.derivation_id
          AND derivation.commit_id = NEW.commit_id
          AND derivation.derivation_name = NEW.configuration_name
          AND derivation.derivation_type = 'nixos'
    ) THEN
        RAISE EXCEPTION
            'retained generation requires NixOS derivation lineage';
    END IF;
    IF NEW.lineage_verified AND NOT EXISTS (
        SELECT 1
        FROM derivations derivation
        WHERE derivation.id = NEW.derivation_id
          AND NEW.source_store_path IS NOT NULL
          AND btrim(NEW.source_store_path) <> ''
          AND NEW.source_store_path = COALESCE(
              derivation.store_path, derivation.expected_store_path
          )
    ) THEN
        RAISE EXCEPTION
            'verified retained generation requires exact store-path lineage';
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER evaluation_generation_artifact_immutable
BEFORE INSERT OR UPDATE ON evaluation_generation_snapshots
FOR EACH ROW EXECUTE FUNCTION enforce_retained_evaluation_artifact();

ALTER TABLE evaluation_generation_snapshots
    ALTER COLUMN lineage_verified SET DEFAULT true;

-- COMPATIBILITY: Existing deployments remain false because their exact
-- artifact cannot be reconstructed. Deployments inserted after this migration
-- opt into reciprocal artifact binding for evaluator/deployment commit races.
ALTER TABLE pending_system_deployments
    ALTER COLUMN evaluation_snapshot_binding_expected SET DEFAULT true;

-- Archived commit reclamation uses the existing ON DELETE SET NULL foreign key.
-- Preserve immutable request intent during its active and ingestion lifetimes,
-- then permit only that FK cleanup after all exact bindings have released.
CREATE OR REPLACE FUNCTION preserve_pending_deployment_identity()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    IF OLD.requested_commit_id IS NOT NULL
       AND NEW.requested_commit_id IS DISTINCT FROM OLD.requested_commit_id
       AND NOT (
           NEW.requested_commit_id IS NULL
           AND NEW.evaluation_snapshot_id IS NULL
           AND NEW.requested_derivation_id IS NULL
           AND NEW.status IN ('succeeded', 'failed', 'expired', 'superseded')
           AND NEW.completed_at <= now() - INTERVAL '24 hours'
       ) THEN
        RAISE EXCEPTION 'requested deployment commit identity is immutable';
    END IF;
    IF OLD.request_identity IS NOT NULL
       AND NEW.request_identity IS DISTINCT FROM OLD.request_identity THEN
        RAISE EXCEPTION 'deployment request identity is immutable';
    END IF;
    IF OLD.request_action IS NOT NULL
       AND NEW.request_action IS DISTINCT FROM OLD.request_action THEN
        RAISE EXCEPTION 'deployment request action is immutable';
    END IF;
    RETURN NEW;
END;
$$;

CREATE FUNCTION preserve_pending_deployment_evaluation_artifact()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP = 'INSERT' AND NOT NEW.evaluation_snapshot_binding_expected THEN
        RAISE EXCEPTION 'new deployments require evaluation artifact binding';
    END IF;
    IF TG_OP = 'UPDATE'
       AND NEW.evaluation_snapshot_binding_expected IS DISTINCT FROM
           OLD.evaluation_snapshot_binding_expected THEN
        RAISE EXCEPTION 'deployment evaluation binding policy is immutable';
    END IF;
    IF TG_OP = 'UPDATE'
       AND OLD.evaluation_snapshot_id IS NOT NULL
       AND NEW.evaluation_snapshot_id IS DISTINCT FROM OLD.evaluation_snapshot_id
       AND NOT (
           NEW.evaluation_snapshot_id IS NULL
           AND NEW.status IN ('succeeded', 'failed', 'expired', 'superseded')
           AND NEW.completed_at <= now() - INTERVAL '24 hours'
       ) THEN
        RAISE EXCEPTION 'deployment evaluation artifact identity is immutable';
    END IF;
    IF TG_OP = 'UPDATE'
       AND OLD.requested_derivation_id IS NOT NULL
       AND NEW.requested_derivation_id IS DISTINCT FROM OLD.requested_derivation_id
       AND NOT (
           NEW.requested_derivation_id IS NULL
           AND NEW.evaluation_snapshot_id IS NULL
           AND NEW.status IN ('succeeded', 'failed', 'expired', 'superseded')
           AND NEW.completed_at <= now() - INTERVAL '24 hours'
       ) THEN
        RAISE EXCEPTION 'deployment derivation identity is immutable';
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER pending_deployment_evaluation_artifact_immutable
BEFORE INSERT OR UPDATE OF evaluation_snapshot_id,
    requested_derivation_id, evaluation_snapshot_binding_expected
    ON pending_system_deployments
FOR EACH ROW EXECUTE FUNCTION preserve_pending_deployment_evaluation_artifact();

-- Validate all deferred lineage constraints before creating indexes. PostgreSQL
-- rejects CREATE INDEX while the backfill has pending constraint-trigger events.
SET CONSTRAINTS ALL IMMEDIATE;

-- PostgreSQL does not create indexes on the referencing side of foreign keys.
-- These indexes keep selector advancement, artifact GC, and retained-history
-- deletion from scanning complete child tables while they enforce RESTRICT.
CREATE INDEX evaluation_snapshot_selections_snapshot_idx
    ON evaluation_snapshot_selections(current_snapshot_id);
CREATE INDEX evaluation_generation_snapshots_snapshot_idx
    ON evaluation_generation_snapshots(snapshot_id);
CREATE INDEX evaluation_generation_snapshots_derivation_idx
    ON evaluation_generation_snapshots(derivation_id);
CREATE INDEX evaluation_generation_snapshots_commit_idx
    ON evaluation_generation_snapshots(commit_id);
CREATE INDEX pending_system_deployments_evaluation_snapshot_idx
    ON pending_system_deployments(evaluation_snapshot_id)
    WHERE evaluation_snapshot_id IS NOT NULL;
CREATE INDEX pending_system_deployments_requested_derivation_idx
    ON pending_system_deployments(requested_derivation_id)
    WHERE requested_derivation_id IS NOT NULL;

COMMENT ON TABLE evaluation_snapshots IS
    'Immutable evaluation attempt artifacts. Core fields and option references never change after insertion; host_delta_count is mutable derived same-commit metadata.';
COMMENT ON TABLE evaluation_snapshot_selections IS
    'Atomic current-attempt selector for one exact commit and configuration lineage. Retained generations do not follow this pointer.';
COMMENT ON TABLE evaluation_snapshot_options IS
    'Immutable option-to-content references owned by one immutable evaluation artifact. References are removed only by cascading artifact reclamation.';
COMMENT ON TABLE evaluation_option_contents IS
    'Immutable redacted content addressed by its safe digest. Unreferenced rows remain eligible for coordinated garbage collection.';
COMMENT ON TABLE evaluation_generation_snapshots IS
    'Retained system generations reference a successful evaluation artifact. lineage_verified distinguishes exact post-0248 deployment/store lineage from queryable legacy rows that are ineligible for rollback. RESTRICT prevents artifact deletion.';
COMMENT ON COLUMN evaluation_snapshots.snapshot_version IS
    'Legacy artifact format version. Immutable artifacts use UUID identity as the continuation token source.';
COMMENT ON COLUMN evaluation_snapshots.integrity_version IS
    'Version 1 certifies that every immutable option payload passed complete SafeOptionValue and provenance validation after all references persisted. Zero is never advertised available.';
COMMENT ON COLUMN pending_system_deployments.evaluation_snapshot_id IS
    'Exact successful evaluation artifact selected when the deployment commit and derivation target were bound. Retained generations inherit this identity. Terminal deployments release the binding after the 24-hour ingestion window.';
COMMENT ON COLUMN pending_system_deployments.evaluation_snapshot_binding_expected IS
    'True only for deployments created after immutable artifacts were introduced. False legacy deployments cannot be promoted to an exact artifact after migration.';
COMMENT ON COLUMN pending_system_deployments.requested_derivation_id IS
    'Exact NixOS derivation selected for a post-0248 deployment. Null supports legacy or path-only deployments whose exact derivation was not established. Terminal deployments release this identity after the ingestion window, including when no available artifact was bound.';

-- Materialized host metrics follow only current selectors. Replaced artifacts
-- retain their last metric for generation history and never re-enter the corpus.
CREATE OR REPLACE FUNCTION recompute_evaluation_host_deltas(target_commit_id integer)
RETURNS void AS $$
BEGIN
    UPDATE evaluation_snapshots snapshot
    SET host_delta_count = NULL
    FROM evaluation_snapshot_selections selection
    WHERE selection.current_snapshot_id = snapshot.id
      AND selection.commit_id = target_commit_id;

    WITH usable_snapshots AS (
        SELECT snapshot.id
        FROM evaluation_snapshot_selections selection
        JOIN evaluation_snapshots snapshot ON snapshot.id = selection.current_snapshot_id
        WHERE selection.commit_id = target_commit_id
          AND snapshot.lifecycle = 'available'
          AND snapshot.schema_version = 1
          AND snapshot.integrity_version = 1
    ), corpus_size AS (
        SELECT COUNT(*)::bigint AS value FROM usable_snapshots
    ), paths AS (
        SELECT DISTINCT item.option_path
        FROM usable_snapshots snapshot
        JOIN evaluation_snapshot_options item ON item.snapshot_id = snapshot.id
    ), present_votes AS (
        SELECT item.option_path, item.content_digest, COUNT(*)::bigint AS votes,
               '1:' || encode(item.content_digest, 'hex') AS state_identity
        FROM usable_snapshots snapshot
        JOIN evaluation_snapshot_options item ON item.snapshot_id = snapshot.id
        GROUP BY item.option_path, item.content_digest
    ), votes AS (
        SELECT option_path, content_digest, votes, state_identity FROM present_votes
        UNION ALL
        SELECT path.option_path, NULL::bytea,
               corpus.value - COALESCE(present.value, 0), '0:'
        FROM paths path
        CROSS JOIN corpus_size corpus
        LEFT JOIN (
            SELECT option_path, SUM(votes)::bigint AS value
            FROM present_votes GROUP BY option_path
        ) present USING (option_path)
        WHERE corpus.value - COALESCE(present.value, 0) > 0
    ), modal AS (
        SELECT option_path, content_digest
        FROM (
            SELECT votes.*,
                   ROW_NUMBER() OVER (
                       PARTITION BY option_path
                       ORDER BY votes DESC, state_identity COLLATE "C"
                   ) AS position
            FROM votes
        ) ranked
        WHERE position = 1
    ), deltas AS (
        SELECT snapshot.id,
               COUNT(*) FILTER (
                   WHERE selected.content_digest IS DISTINCT FROM modal.content_digest
               )::bigint AS host_delta_count
        FROM usable_snapshots snapshot
        LEFT JOIN modal ON true
        LEFT JOIN evaluation_snapshot_options selected
          ON selected.snapshot_id = snapshot.id
         AND selected.option_path = modal.option_path
        GROUP BY snapshot.id
    )
    UPDATE evaluation_snapshots snapshot
    SET host_delta_count = deltas.host_delta_count
    FROM deltas
    WHERE snapshot.id = deltas.id;
END;
$$ LANGUAGE plpgsql;

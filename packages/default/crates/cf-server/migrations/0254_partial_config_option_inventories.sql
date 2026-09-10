-- TASK-440: represent complete and partial schema-V2 Config option inventories.
--
-- Before this migration, the V2 index job failed when option-tree enumeration
-- failed. Reconciliation then required every indexed metadata and value job,
-- and certification required option_count to equal all persisted references.
-- Therefore every existing V2 artifact necessarily represents a complete
-- enumeration. V1 rows have no inventory claim and remain NULL.

ALTER TABLE evaluation_snapshots
    ADD COLUMN option_inventory_complete boolean,
    ADD COLUMN option_inventory_diagnostics jsonb,
    ADD COLUMN option_inventory_diagnostics_truncated boolean,
    ADD CONSTRAINT evaluation_snapshots_v1_inventory_state_check
        CHECK (schema_version <> 1 OR (
            option_inventory_complete IS NULL
            AND option_inventory_diagnostics IS NULL
            AND option_inventory_diagnostics_truncated IS NULL
        ));

UPDATE evaluation_snapshots
SET option_inventory_complete = TRUE,
    option_inventory_diagnostics = '[]'::jsonb,
    option_inventory_diagnostics_truncated = FALSE
WHERE schema_version = 2;

CREATE FUNCTION evaluation_option_inventory_diagnostics_v2_valid(
    candidate jsonb,
    complete boolean,
    truncated boolean
)
RETURNS boolean LANGUAGE plpgsql IMMUTABLE AS $$
DECLARE
    diagnostic jsonb;
    component jsonb;
    previous_path_key bytea;
    current_path_key bytea;
BEGIN
    IF candidate IS NULL OR complete IS NULL OR truncated IS NULL
       OR jsonb_typeof(candidate) <> 'array'
       OR jsonb_array_length(candidate) > 128
       OR complete IS DISTINCT FROM (
           jsonb_array_length(candidate) = 0 AND NOT truncated
       )
       OR (NOT complete AND jsonb_array_length(candidate) = 0) THEN
        RETURN false;
    END IF;
    FOR diagnostic IN SELECT value FROM jsonb_array_elements(candidate) LOOP
        IF jsonb_typeof(diagnostic) <> 'object'
           OR NOT diagnostic ?& ARRAY['path', 'code', 'message']
           OR diagnostic - ARRAY['path', 'code', 'message'] <> '{}'::jsonb
           OR jsonb_typeof(diagnostic->'path') <> 'array'
           OR jsonb_array_length(diagnostic->'path') NOT BETWEEN 1 AND 16
           OR jsonb_typeof(diagnostic->'code') <> 'string'
           OR jsonb_typeof(diagnostic->'message') <> 'string' THEN
            RETURN false;
        END IF;
        FOR component IN SELECT value FROM jsonb_array_elements(diagnostic->'path') LOOP
            IF jsonb_typeof(component) <> 'string'
               OR component #>> '{}' = ''
               OR char_length(component #>> '{}') > 256 THEN
                RETURN false;
            END IF;
        END LOOP;
        IF (diagnostic->>'code' = 'unreadable_option_subtree'
            AND diagnostic->>'message' = 'Option subtree could not be inspected')
           OR (diagnostic->>'code' = 'option_subtree_depth_exceeded'
            AND diagnostic->>'message' = 'Option subtree exceeds the traversal depth limit') THEN
            NULL;
        ELSE
            RETURN false;
        END IF;
        -- INVARIANT: UTF-8 bytes of the canonical JSON array match the Nix and
        -- Rust diagnostic sort key independently of the database collation.
        current_path_key := convert_to((diagnostic->'path')::text, 'UTF8');
        IF previous_path_key IS NOT NULL AND previous_path_key >= current_path_key THEN
            RETURN false;
        END IF;
        previous_path_key := current_path_key;
    END LOOP;
    RETURN true;
EXCEPTION WHEN OTHERS THEN
    RETURN false;
END;
$$;

CREATE OR REPLACE FUNCTION evaluation_snapshot_payloads_valid(target_snapshot_id uuid)
RETURNS boolean LANGUAGE plpgsql STABLE STRICT AS $$
DECLARE
    snapshot evaluation_snapshots%ROWTYPE;
    global_state text;
    enrichment_state text;
    provenance_ready boolean;
    item record;
    definition jsonb;
    module_count integer;
BEGIN
    SELECT * INTO snapshot FROM evaluation_snapshots WHERE id = target_snapshot_id;
    IF NOT FOUND OR snapshot.lifecycle <> 'available' THEN RETURN false; END IF;

    IF snapshot.schema_version = 1 THEN
        RETURN EXISTS (
            SELECT 1 FROM evaluation_snapshots s
            WHERE s.id = target_snapshot_id
              AND s.option_count = (SELECT count(*) FROM evaluation_snapshot_options WHERE snapshot_id = s.id)
              AND s.module_count = (
                  SELECT count(DISTINCT (d.value->>'source_input', d.value->>'source_revision', d.value->>'source_path'))
                  FROM evaluation_snapshot_options o
                  JOIN evaluation_option_contents c ON c.digest = o.content_digest
                  CROSS JOIN LATERAL jsonb_array_elements(CASE WHEN jsonb_typeof(c.payload->'definitions') = 'array' THEN c.payload->'definitions' ELSE '[]'::jsonb END) d(value)
                  WHERE o.snapshot_id = s.id
              )
              AND NOT EXISTS (
                  SELECT 1 FROM evaluation_snapshot_options o
                  LEFT JOIN evaluation_option_contents c ON c.digest = o.content_digest
                   WHERE o.snapshot_id = s.id AND (o.option_key IS NOT NULL
                      OR o.path_components IS NOT NULL OR c.digest IS NULL OR c.schema_version <> 1
                      OR NOT evaluation_option_payload_valid(c.payload)
                      OR c.payload->'overridden' IS DISTINCT FROM to_jsonb(o.is_overridden))
              )
        );
    ELSIF snapshot.schema_version <> 2 THEN
        RETURN false;
    END IF;

    IF snapshot.target_key IS NULL OR snapshot.target_key !~ '^[0-9a-f]{64}$'
       OR snapshot.source_out_path IS NULL OR btrim(snapshot.source_out_path) = ''
       OR snapshot.carrier_drv_path IS NULL OR btrim(snapshot.carrier_drv_path) = ''
       OR snapshot.provenance_state IS NULL OR snapshot.comparison_ready IS NULL
        OR NOT evaluation_option_inventory_diagnostics_v2_valid(
            snapshot.option_inventory_diagnostics,
            snapshot.option_inventory_complete,
            snapshot.option_inventory_diagnostics_truncated
       )
       OR (NOT snapshot.option_inventory_complete AND snapshot.comparison_ready) THEN
        RETURN false;
    END IF;
    global_state := snapshot.provenance_state->>'state';
    enrichment_state := snapshot.provenance_state->'definition_value_enrichment'->>'state';
    provenance_ready := global_state = 'available' AND enrichment_state = 'available';
    IF NOT evaluation_snapshot_v2_provenance_valid(snapshot.provenance_state, provenance_ready)
       OR snapshot.comparison_ready IS DISTINCT FROM (
           snapshot.option_inventory_complete AND provenance_ready
       ) THEN
        RETURN false;
    END IF;
    IF snapshot.option_count <> (SELECT count(*) FROM evaluation_snapshot_options WHERE snapshot_id = snapshot.id)
       OR (global_state = 'unavailable' AND snapshot.module_count <> 0) THEN RETURN false; END IF;
    SELECT count(DISTINCT (d.value->>'source_input', d.value->>'source_revision', d.value->>'source_path'))::integer
    INTO module_count
    FROM evaluation_snapshot_options o
    JOIN evaluation_option_contents c ON c.digest = o.content_digest
    CROSS JOIN LATERAL jsonb_array_elements(c.payload->'provenance'->'definitions') d(value)
    WHERE o.snapshot_id = snapshot.id AND global_state = 'available'
      AND (d.value->>'source_input' IS NOT NULL OR d.value->>'source_revision' IS NOT NULL OR d.value->>'source_path' IS NOT NULL);
    IF snapshot.module_count <> COALESCE(module_count, 0) THEN RETURN false; END IF;
    FOR item IN SELECT o.*, c.schema_version, c.payload
        FROM evaluation_snapshot_options o
        LEFT JOIN evaluation_option_contents c ON c.digest = o.content_digest
        WHERE o.snapshot_id = snapshot.id LOOP
        IF item.option_key IS NULL OR item.path_components IS NULL
           OR item.schema_version <> 2 OR NOT evaluation_option_payload_v2_valid(item.payload) THEN RETURN false; END IF;
        IF global_state = 'available' THEN
            IF item.payload->'provenance'->>'state' <> 'available'
               OR item.is_overridden IS NULL
               OR item.payload->'provenance'->>'override_state' <> item.is_overridden::text THEN RETURN false; END IF;
            FOR definition IN SELECT value FROM jsonb_array_elements(item.payload->'provenance'->'definitions') LOOP
                IF enrichment_state = 'available'
                   AND (definition->'value' IS NULL OR jsonb_typeof(definition->'value') = 'null') THEN RETURN false; END IF;
                IF enrichment_state = 'unavailable' AND definition->'value' IS NOT NULL
                   AND jsonb_typeof(definition->'value') <> 'null' THEN RETURN false; END IF;
            END LOOP;
        ELSIF item.payload->'provenance'->>'state' <> 'unavailable' OR item.is_overridden IS NOT NULL THEN RETURN false;
        END IF;
    END LOOP;
    RETURN true;
EXCEPTION WHEN OTHERS THEN
    RETURN false;
END;
$$;

CREATE OR REPLACE FUNCTION preserve_evaluation_snapshot_artifact()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP = 'INSERT' THEN
        IF NEW.integrity_version <> 0 THEN RAISE EXCEPTION 'evaluation snapshot integrity must be certified after insertion'; END IF;
        RETURN NEW;
    END IF;
    IF NEW.id IS DISTINCT FROM OLD.id OR NEW.commit_id IS DISTINCT FROM OLD.commit_id
       OR NEW.configuration_name IS DISTINCT FROM OLD.configuration_name
       OR NEW.schema_version IS DISTINCT FROM OLD.schema_version OR NEW.lifecycle IS DISTINCT FROM OLD.lifecycle
       OR NEW.first_parent_sha IS DISTINCT FROM OLD.first_parent_sha OR NEW.error IS DISTINCT FROM OLD.error
       OR NEW.option_count IS DISTINCT FROM OLD.option_count OR NEW.module_count IS DISTINCT FROM OLD.module_count
       OR NEW.evaluation_duration_ms IS DISTINCT FROM OLD.evaluation_duration_ms OR NEW.content_bytes IS DISTINCT FROM OLD.content_bytes
       OR NEW.created_at IS DISTINCT FROM OLD.created_at OR NEW.completed_at IS DISTINCT FROM OLD.completed_at
       OR NEW.snapshot_version IS DISTINCT FROM OLD.snapshot_version
       OR NEW.target_key IS DISTINCT FROM OLD.target_key OR NEW.source_out_path IS DISTINCT FROM OLD.source_out_path
       OR NEW.carrier_drv_path IS DISTINCT FROM OLD.carrier_drv_path OR NEW.provenance_state IS DISTINCT FROM OLD.provenance_state
       OR NEW.comparison_ready IS DISTINCT FROM OLD.comparison_ready
       OR NEW.option_inventory_complete IS DISTINCT FROM OLD.option_inventory_complete
        OR NEW.option_inventory_diagnostics IS DISTINCT FROM OLD.option_inventory_diagnostics
        OR NEW.option_inventory_diagnostics_truncated IS DISTINCT FROM OLD.option_inventory_diagnostics_truncated
       OR (NEW.integrity_version IS DISTINCT FROM OLD.integrity_version AND NOT (
           OLD.integrity_version = 0 AND NEW.integrity_version = NEW.schema_version
           AND NEW.schema_version IN (1, 2) AND evaluation_snapshot_payloads_valid(NEW.id)
       )) THEN RAISE EXCEPTION 'evaluation snapshot artifacts are immutable'; END IF;
    RETURN NEW;
END;
$$;

COMMENT ON COLUMN evaluation_snapshots.option_inventory_complete IS
    'Schema-V2 claim that option_count covers the complete option tree. False requires bounded diagnostics and comparison_ready false. NULL is required for schema V1.';
COMMENT ON COLUMN evaluation_snapshots.option_inventory_diagnostics IS
    'Schema-V2 bounded redacted deterministic unreadable-prefix diagnostics. Complete inventories use the canonical empty array; schema V1 uses NULL.';
COMMENT ON COLUMN evaluation_snapshots.option_inventory_diagnostics_truncated IS
    'Schema-V2 certification that diagnostic detail was omitted by the retention bound or post-redaction deduplication. Complete inventories use false; schema V1 uses NULL.';

-- TASK-440: add the additive PostgreSQL contract for config snapshot artifacts V2.
--
-- V1 rows remain unchanged and remain the only rows advertised by current
-- readers. V2 persistence and read paths are intentionally deferred. The
-- host-delta function also remains V1-only until a schema-aware reader exists.

ALTER TABLE evaluation_option_contents
    DROP CONSTRAINT evaluation_option_contents_schema_version_check;

ALTER TABLE evaluation_snapshots
    DROP CONSTRAINT evaluation_snapshots_schema_version_check,
    DROP CONSTRAINT evaluation_snapshots_integrity_version_check;

ALTER TABLE evaluation_option_contents
    ADD CONSTRAINT evaluation_option_contents_schema_version_check
        CHECK (schema_version IN (1, 2));

ALTER TABLE evaluation_snapshots
    ADD CONSTRAINT evaluation_snapshots_schema_version_check
        CHECK (schema_version IN (1, 2)),
    ADD CONSTRAINT evaluation_snapshots_integrity_version_check
        CHECK (integrity_version IN (0, 1, 2)),
    ADD COLUMN target_key text,
    ADD COLUMN source_out_path text,
    ADD COLUMN carrier_drv_path text,
    ADD COLUMN provenance_state jsonb,
    ADD COLUMN comparison_ready boolean,
    ADD CONSTRAINT evaluation_snapshots_target_key_check
        CHECK (target_key IS NULL OR target_key ~ '^[0-9a-f]{64}$'),
    ADD CONSTRAINT evaluation_snapshots_source_out_path_check
        CHECK (source_out_path IS NULL OR btrim(source_out_path) <> ''),
    ADD CONSTRAINT evaluation_snapshots_carrier_drv_path_check
        CHECK (carrier_drv_path IS NULL OR btrim(carrier_drv_path) <> ''),
    ADD CONSTRAINT evaluation_snapshots_provenance_object_check
        CHECK (provenance_state IS NULL OR jsonb_typeof(provenance_state) = 'object'),
    ADD CONSTRAINT evaluation_snapshots_v1_global_state_check
        CHECK (schema_version <> 1 OR (
            target_key IS NULL AND source_out_path IS NULL
            AND carrier_drv_path IS NULL AND provenance_state IS NULL
            AND comparison_ready IS NULL
        ));

ALTER TABLE evaluation_snapshot_options
    ADD COLUMN option_key text,
    ADD COLUMN path_components text[],
    ALTER COLUMN is_overridden DROP NOT NULL,
    -- SAFETY: All production V1 writers explicitly supply is_overridden.
    -- Verified: evaluation_snapshots.rs INSERT statements at lines 341, 6984, 9264, 9303.
    ALTER COLUMN is_overridden DROP DEFAULT,
    ADD CONSTRAINT evaluation_snapshot_options_v2_identity_pair_check
        CHECK ((option_key IS NULL) = (path_components IS NULL)),
    ADD CONSTRAINT evaluation_snapshot_options_option_key_check
        CHECK (option_key IS NULL OR option_key ~ '^[0-9a-f]{64}$'),
    ADD CONSTRAINT evaluation_snapshot_options_path_components_check
        CHECK (path_components IS NULL OR (
            cardinality(path_components) > 0
            AND array_position(path_components, NULL) IS NULL
            AND array_position(path_components, '') IS NULL
        ));

CREATE UNIQUE INDEX evaluation_snapshot_options_v2_key_idx
    ON evaluation_snapshot_options(snapshot_id, option_key)
    WHERE option_key IS NOT NULL;

CREATE UNIQUE INDEX evaluation_snapshot_options_v2_path_idx
    ON evaluation_snapshot_options(snapshot_id, path_components)
    WHERE path_components IS NOT NULL;

COMMENT ON COLUMN evaluation_snapshot_options.option_key IS
    'Authoritative schema-V2 option identity. The semantic pipeline validates its correspondence to path_components before persistence.';
COMMENT ON COLUMN evaluation_snapshot_options.path_components IS
    'Authoritative schema-V2 option identity. option_path remains a denormalized compatibility and search projection.';
COMMENT ON COLUMN evaluation_snapshot_options.option_path IS
    'Legacy denormalized compatibility and search projection; schema-V2 identity is option_key plus path_components.';

CREATE FUNCTION evaluation_safe_error_v2_valid(candidate jsonb)
RETURNS boolean LANGUAGE plpgsql IMMUTABLE STRICT AS $$
BEGIN
    -- INVARIANT: Total boolean validator. Returns FALSE for malformed input, never NULL.
    -- Required keys: code (string), message (string).
    RETURN COALESCE(
        jsonb_typeof(candidate) = 'object'
        AND candidate ?& ARRAY['code', 'message']
        AND jsonb_typeof(candidate->'code') = 'string'
        AND jsonb_typeof(candidate->'message') = 'string',
        false
    );
EXCEPTION WHEN OTHERS THEN
    RETURN false;
END;
$$;

CREATE FUNCTION evaluation_definition_source_v2_valid(candidate jsonb)
RETURNS boolean LANGUAGE plpgsql IMMUTABLE STRICT AS $$
BEGIN
    -- INVARIANT: Total boolean validator. Returns FALSE for malformed input, never NULL.
    -- Required keys: source_path (string), priority (number|null), source_input (string|null), source_revision (string|null).
    RETURN COALESCE(
        jsonb_typeof(candidate) = 'object'
        AND candidate ?& ARRAY['source_path', 'priority', 'source_input', 'source_revision']
        AND jsonb_typeof(candidate->'source_path') = 'string'
        AND jsonb_typeof(candidate->'priority') IN ('number', 'null')
        AND (jsonb_typeof(candidate->'priority') = 'null'
             OR (candidate->>'priority')::bigint::text = candidate->>'priority')
        AND jsonb_typeof(candidate->'source_input') IN ('string', 'null')
        AND jsonb_typeof(candidate->'source_revision') IN ('string', 'null'),
        false
    );
EXCEPTION WHEN OTHERS THEN
    RETURN false;
END;
$$;

CREATE FUNCTION evaluation_option_payload_v2_valid(candidate jsonb)
RETURNS boolean LANGUAGE plpgsql IMMUTABLE STRICT AS $$
DECLARE
    definition jsonb;
    expected_ordinal bigint := 0;
    merge_orders bigint[] := ARRAY[]::bigint[];
    survivor_count bigint := 0;
    has_discarded boolean := false;
    status text;
BEGIN
    -- INVARIANT: Total boolean validator. Returns FALSE for malformed input, never NULL.
    -- Required top-level keys: metadata, effective_value, provenance.
    IF jsonb_typeof(candidate) <> 'object'
       OR NOT candidate ?& ARRAY['metadata', 'effective_value', 'provenance']
       OR jsonb_typeof(candidate->'metadata') <> 'object'
       OR jsonb_typeof(candidate->'effective_value') <> 'object'
       OR jsonb_typeof(candidate->'provenance') <> 'object' THEN
        RETURN false;
    END IF;

    IF NOT candidate->'metadata' ? 'state' THEN
        RETURN false;
    ELSIF (candidate->'metadata'->>'state') = 'available' THEN
        IF NOT candidate->'metadata' ?& ARRAY[
               'option_type', 'loc', 'declared_type', 'declarations',
               'declaration_positions', 'highest_prio', 'is_defined',
               'surviving_definition_sources'
           ]
           OR jsonb_typeof(candidate->'metadata'->'option_type') NOT IN ('string', 'null')
           OR jsonb_typeof(candidate->'metadata'->'loc') <> 'array'
           OR jsonb_typeof(candidate->'metadata'->'declared_type') NOT IN ('string', 'null')
           OR jsonb_typeof(candidate->'metadata'->'declarations') <> 'array'
           OR jsonb_typeof(candidate->'metadata'->'declaration_positions') <> 'array'
           OR jsonb_typeof(candidate->'metadata'->'highest_prio') NOT IN ('number', 'null')
           OR jsonb_typeof(candidate->'metadata'->'is_defined') <> 'boolean'
           OR jsonb_typeof(candidate->'metadata'->'surviving_definition_sources') <> 'array'
           OR EXISTS (SELECT 1 FROM jsonb_array_elements(candidate->'metadata'->'surviving_definition_sources') x
                      WHERE NOT evaluation_definition_source_v2_valid(x))
           OR EXISTS (SELECT 1 FROM jsonb_array_elements(candidate->'metadata'->'loc') x
                      WHERE jsonb_typeof(x) <> 'string')
           OR EXISTS (SELECT 1 FROM jsonb_array_elements(candidate->'metadata'->'declarations') x
                      WHERE jsonb_typeof(x) <> 'string') THEN
            RETURN false;
        END IF;
        IF candidate->'metadata'->'highest_prio' IS NOT NULL
           AND jsonb_typeof(candidate->'metadata'->'highest_prio') = 'number'
           AND (candidate->'metadata'->>'highest_prio')::bigint::text
               <> candidate->'metadata'->>'highest_prio' THEN
            RETURN false;
        END IF;
    ELSIF (candidate->'metadata'->>'state') = 'failed' THEN
        IF NOT candidate->'metadata' ? 'error'
           OR NOT evaluation_safe_error_v2_valid(candidate->'metadata'->'error') THEN
            RETURN false;
        END IF;
    ELSE
        RETURN false;
    END IF;

    IF NOT evaluation_safe_option_value_valid(candidate->'effective_value') THEN
        RETURN false;
    END IF;

    IF NOT candidate->'provenance' ? 'state' THEN
        RETURN false;
    ELSIF (candidate->'provenance'->>'state') = 'unavailable' THEN
        IF candidate->'provenance' ? 'definitions'
           OR candidate->'provenance' ? 'override_state' THEN
            RETURN false;
        END IF;
        RETURN true;
    ELSIF (candidate->'provenance'->>'state') <> 'available'
       OR NOT candidate->'provenance' ?& ARRAY['definitions', 'override_state']
       OR jsonb_typeof(candidate->'provenance'->'definitions') <> 'array'
       OR jsonb_typeof(candidate->'provenance'->'override_state') <> 'boolean' THEN
        RETURN false;
    END IF;

    FOR definition IN SELECT value FROM jsonb_array_elements(candidate->'provenance'->'definitions') LOOP
        IF NOT definition ?& ARRAY[
               'ordinal', 'source_path', 'source_input', 'source_revision',
               'module_key', 'priority', 'status', 'surviving_merge_order', 'value'
           ]
           OR jsonb_typeof(definition->'ordinal') <> 'number'
           OR (definition->>'ordinal')::bigint < 0
           OR (definition->>'ordinal')::bigint::text <> definition->>'ordinal'
           OR (definition->>'ordinal')::bigint <> expected_ordinal
           OR jsonb_typeof(definition->'source_path') NOT IN ('string', 'null')
           OR jsonb_typeof(definition->'source_input') NOT IN ('string', 'null')
           OR jsonb_typeof(definition->'source_revision') NOT IN ('string', 'null')
           OR jsonb_typeof(definition->'module_key') NOT IN ('string', 'null')
           OR jsonb_typeof(definition->'priority') <> 'number'
           OR (definition->>'priority')::bigint::text <> definition->>'priority'
           OR (definition->>'status') NOT IN ('active_surviving', 'priority_discarded')
           OR jsonb_typeof(definition->'surviving_merge_order') NOT IN ('number', 'null')
           OR (definition->'value' IS NOT NULL
               AND jsonb_typeof(definition->'value') <> 'null'
               AND NOT evaluation_safe_option_value_valid(definition->'value')) THEN
            RETURN false;
        END IF;
        status := definition->>'status';
        IF status = 'active_surviving' THEN
            IF jsonb_typeof(definition->'surviving_merge_order') <> 'number'
               OR (definition->>'surviving_merge_order')::bigint < 0
               OR (definition->>'surviving_merge_order')::bigint::text
                  <> definition->>'surviving_merge_order' THEN
                RETURN false;
            END IF;
            merge_orders := array_append(
                merge_orders, (definition->>'surviving_merge_order')::bigint
            );
            survivor_count := survivor_count + 1;
        ELSIF definition->'surviving_merge_order' IS NOT NULL
           AND jsonb_typeof(definition->'surviving_merge_order') <> 'null' THEN
            RETURN false;
        ELSE
            has_discarded := true;
        END IF;
        expected_ordinal := expected_ordinal + 1;
    END LOOP;
    IF (candidate->'provenance'->>'override_state')::boolean IS DISTINCT FROM has_discarded THEN
        RETURN false;
    END IF;
    IF survivor_count > 0 THEN
        IF cardinality(merge_orders) <> survivor_count
           OR (SELECT count(DISTINCT value) FROM unnest(merge_orders) values(value))
              <> survivor_count THEN
            RETURN false;
        END IF;
        FOR expected_ordinal IN 0..survivor_count - 1 LOOP
            IF NOT expected_ordinal = ANY(merge_orders) THEN
                RETURN false;
            END IF;
        END LOOP;
    END IF;
    RETURN true;
EXCEPTION WHEN OTHERS THEN
    RETURN false;
END;
$$;

CREATE FUNCTION evaluation_snapshot_v2_provenance_valid(candidate jsonb, ready boolean)
RETURNS boolean LANGUAGE plpgsql IMMUTABLE AS $$
DECLARE
    enrichment jsonb;
BEGIN
    IF candidate IS NULL OR jsonb_typeof(candidate) <> 'object'
       OR NOT candidate ? 'state'
       OR (candidate->>'state') NOT IN ('available', 'unavailable') THEN
        RETURN false;
    END IF;
    IF candidate->>'state' = 'unavailable' THEN
        RETURN candidate ?& ARRAY['reason_code', 'diagnostic']
           AND jsonb_typeof(candidate->'reason_code') = 'string'
           AND (jsonb_typeof(candidate->'diagnostic') = 'null'
                OR evaluation_safe_error_v2_valid(candidate->'diagnostic'))
           AND ready IS FALSE;
    END IF;
    IF NOT candidate ?& ARRAY[
           'adapter_version', 'target_lib_version',
           'target_module_system_path', 'provenance_digest',
           'definition_value_enrichment'
       ]
       OR jsonb_typeof(candidate->'adapter_version') <> 'number'
       OR (candidate->>'adapter_version')::bigint < 0
       OR jsonb_typeof(candidate->'provenance_digest') <> 'string'
       OR candidate->>'provenance_digest' !~ '^[0-9a-f]{64}$'
       OR jsonb_typeof(candidate->'target_lib_version') NOT IN ('string', 'null')
       OR jsonb_typeof(candidate->'target_module_system_path') NOT IN ('string', 'null')
       OR jsonb_typeof(candidate->'definition_value_enrichment') <> 'object' THEN
        RETURN false;
    END IF;
    enrichment := candidate->'definition_value_enrichment';
    IF NOT enrichment ? 'state' THEN
        RETURN false;
    ELSIF (enrichment->>'state') = 'available' THEN
        IF NOT enrichment ?& ARRAY['adapter_version', 'provenance_digest']
           OR jsonb_typeof(enrichment->'adapter_version') <> 'number'
           OR (enrichment->>'adapter_version')::bigint < 0
           OR jsonb_typeof(enrichment->'provenance_digest') <> 'string'
           OR enrichment->>'provenance_digest' !~ '^[0-9a-f]{64}$'
           OR (enrichment->>'adapter_version')::bigint
              <> (candidate->>'adapter_version')::bigint
           OR enrichment->>'provenance_digest'
              <> candidate->>'provenance_digest'
           OR ready IS NOT TRUE THEN
            RETURN false;
        END IF;
    ELSIF (enrichment->>'state') = 'unavailable' THEN
        IF NOT enrichment ? 'reason_code'
           OR NOT enrichment ? 'diagnostic'
           OR jsonb_typeof(enrichment->'reason_code') <> 'string'
            OR (NOT enrichment ? 'diagnostic'
                 OR (jsonb_typeof(enrichment->'diagnostic') <> 'null'
                     AND NOT evaluation_safe_error_v2_valid(enrichment->'diagnostic')))
           OR ready IS NOT FALSE THEN
            RETURN false;
        END IF;
    ELSE
        RETURN false;
    END IF;
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

    IF snapshot.target_key IS NULL
       OR snapshot.target_key !~ '^[0-9a-f]{64}$'
       OR snapshot.source_out_path IS NULL
       OR btrim(snapshot.source_out_path) = ''
       OR snapshot.carrier_drv_path IS NULL
       OR btrim(snapshot.carrier_drv_path) = ''
       OR snapshot.provenance_state IS NULL
       OR snapshot.comparison_ready IS NULL THEN
        RETURN false;
    END IF;

    IF NOT evaluation_snapshot_v2_provenance_valid(snapshot.provenance_state, snapshot.comparison_ready) THEN
        RETURN false;
    END IF;
    global_state := snapshot.provenance_state->>'state';
    enrichment_state := snapshot.provenance_state->'definition_value_enrichment'->>'state';
    IF snapshot.option_count <> (SELECT count(*) FROM evaluation_snapshot_options WHERE snapshot_id = snapshot.id)
       OR (global_state = 'unavailable' AND snapshot.module_count <> 0) THEN
        RETURN false;
    END IF;
    SELECT count(DISTINCT (d.value->>'source_input', d.value->>'source_revision', d.value->>'source_path'))::integer
    INTO module_count
    FROM evaluation_snapshot_options o
    JOIN evaluation_option_contents c ON c.digest = o.content_digest
    CROSS JOIN LATERAL jsonb_array_elements(c.payload->'provenance'->'definitions') d(value)
    WHERE o.snapshot_id = snapshot.id
      AND global_state = 'available'
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
                   AND (definition->'value' IS NULL OR jsonb_typeof(definition->'value') = 'null')
                THEN RETURN false; END IF;
                IF enrichment_state = 'unavailable' AND definition->'value' IS NOT NULL
                   AND jsonb_typeof(definition->'value') <> 'null' THEN RETURN false; END IF;
            END LOOP;
        ELSIF item.payload->'provenance'->>'state' <> 'unavailable' OR item.is_overridden IS NOT NULL THEN
            RETURN false;
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
       OR (NEW.integrity_version IS DISTINCT FROM OLD.integrity_version AND NOT (
           OLD.integrity_version = 0 AND NEW.integrity_version = NEW.schema_version
           AND NEW.schema_version IN (1, 2) AND evaluation_snapshot_payloads_valid(NEW.id)
       )) THEN
        RAISE EXCEPTION 'evaluation snapshot artifacts are immutable';
    END IF;
    RETURN NEW;
END;
$$;

CREATE OR REPLACE FUNCTION preserve_evaluation_snapshot_option()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP = 'INSERT' AND EXISTS (
        SELECT 1 FROM evaluation_snapshots snapshot
        WHERE snapshot.id = NEW.snapshot_id AND snapshot.integrity_version <> 0
    ) THEN RAISE EXCEPTION 'certified evaluation snapshot option references are immutable'; END IF;
    IF TG_OP = 'UPDATE' THEN RAISE EXCEPTION 'evaluation snapshot option references are immutable'; END IF;
    IF TG_OP = 'DELETE' AND pg_trigger_depth() = 1 THEN RAISE EXCEPTION 'evaluation snapshot option references are immutable'; END IF;
    RETURN CASE WHEN TG_OP = 'INSERT' THEN NEW ELSE OLD END;
END;
$$;

CREATE OR REPLACE FUNCTION enforce_retained_evaluation_artifact()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP = 'UPDATE' THEN RAISE EXCEPTION 'retained evaluation artifacts are immutable'; END IF;
    IF NOT NEW.lineage_verified THEN RAISE EXCEPTION 'new retained generations require verified deployment lineage'; END IF;
    IF NOT EXISTS (
        SELECT 1 FROM evaluation_snapshots snapshot
        WHERE snapshot.id = NEW.snapshot_id AND snapshot.commit_id = NEW.commit_id
          AND snapshot.configuration_name = NEW.configuration_name AND snapshot.lifecycle = 'available'
          AND snapshot.schema_version IN (1, 2) AND snapshot.integrity_version = snapshot.schema_version
    ) THEN RAISE EXCEPTION 'retained generation requires an exact successful evaluation artifact'; END IF;
    IF NOT EXISTS (
        SELECT 1 FROM derivations derivation
        WHERE derivation.id = NEW.derivation_id AND derivation.commit_id = NEW.commit_id
          AND derivation.derivation_name = NEW.configuration_name AND derivation.derivation_type = 'nixos'
    ) THEN RAISE EXCEPTION 'retained generation requires NixOS derivation lineage'; END IF;
    IF NEW.lineage_verified AND NOT EXISTS (
        SELECT 1 FROM derivations derivation
        WHERE derivation.id = NEW.derivation_id AND NEW.source_store_path IS NOT NULL
          AND btrim(NEW.source_store_path) <> ''
          AND NEW.source_store_path = COALESCE(derivation.store_path, derivation.expected_store_path)
    ) THEN RAISE EXCEPTION 'verified retained generation requires exact store-path lineage'; END IF;
    RETURN NEW;
END;
$$;

COMMENT ON COLUMN evaluation_snapshots.integrity_version IS
    'Artifact certification: 0 is uncertified, 1 certifies schema V1, and 2 certifies schema V2. Certification requires the version-aware payload validator.';
COMMENT ON COLUMN evaluation_snapshots.provenance_state IS
    'Redacted schema-V2 configuration-global provenance and definition-value enrichment state. NULL is required for schema V1.';
COMMENT ON COLUMN evaluation_snapshots.comparison_ready IS
    'Schema-V2 comparison claim, certified only when global provenance and definition-value enrichment are available.';

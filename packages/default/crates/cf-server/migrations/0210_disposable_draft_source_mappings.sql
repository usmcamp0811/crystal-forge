-- Migration 0210: permit transactional removal of source mappings that only
-- target mutable draft versions. Mappings tied to immutable history remain
-- protected even when a lineage hard-delete is otherwise attempted.

CREATE OR REPLACE FUNCTION guard_source_object_mapping_immutability()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP = 'DELETE' THEN
        IF EXISTS (
            SELECT 1
            FROM deployment_policy_versions pv
            WHERE pv.id = OLD.policy_version_id
              AND pv.publication_state IN ('accepted', 'deprecated')
        ) OR EXISTS (
            SELECT 1
            FROM compliance_bundle_versions bv
            WHERE bv.id = OLD.bundle_version_id
              AND bv.publication_state IN ('accepted', 'deprecated')
        ) THEN
            RAISE EXCEPTION
                'Source-object mapping % is immutable because it references immutable history.',
                OLD.id;
        END IF;
        RETURN OLD;
    END IF;

    IF to_jsonb(NEW) IS DISTINCT FROM to_jsonb(OLD) THEN
        RAISE EXCEPTION
            'Source-object mapping % is immutable and cannot be updated.',
            OLD.id;
    END IF;

    RETURN NEW;
END;
$$;

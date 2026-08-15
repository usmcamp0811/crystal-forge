-- Requirement hierarchy construction may change only while the semantic digest
-- is still pending. Once finalized, requirement snapshots are fully immutable.

CREATE OR REPLACE FUNCTION enforce_requirement_version_immutability()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP = 'DELETE' THEN
        RAISE EXCEPTION 'Cannot delete requirement version %.', OLD.id;
    END IF;

    IF OLD.semantic_digest = 'pending' THEN
        IF (to_jsonb(NEW) - 'semantic_digest' - 'parent_requirement_version_id')
           IS DISTINCT FROM
           (to_jsonb(OLD) - 'semantic_digest' - 'parent_requirement_version_id') THEN
            RAISE EXCEPTION 'Cannot modify requirement version % during construction.', OLD.id;
        END IF;
        RETURN NEW;
    END IF;

    RAISE EXCEPTION 'Cannot modify immutable requirement version %.', OLD.id;
END;
$$;

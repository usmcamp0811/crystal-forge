-- Enforce the immutability promised by the normalized framework and
-- requirement-version model at the database boundary.

CREATE OR REPLACE FUNCTION enforce_framework_version_immutability()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP = 'DELETE' THEN
        RAISE EXCEPTION 'Cannot delete framework version %.', OLD.id;
    END IF;

    -- New rows are inserted with a pending digest and finalized immediately by
    -- the writer in the same transaction. No other field may be changed.
    IF OLD.semantic_digest = 'pending'
       AND (to_jsonb(NEW) - 'semantic_digest')
           IS DISTINCT FROM (to_jsonb(OLD) - 'semantic_digest') THEN
        RAISE EXCEPTION 'Cannot modify framework version % while finalizing its digest.', OLD.id;
    END IF;
    IF OLD.semantic_digest <> 'pending'
       OR NEW.semantic_digest IS NULL
       OR NEW.semantic_digest = 'pending' THEN
        RAISE EXCEPTION 'Cannot modify immutable framework version %.', OLD.id;
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER compliance_framework_versions_immutability
BEFORE UPDATE OR DELETE ON compliance_framework_versions
FOR EACH ROW EXECUTE FUNCTION enforce_framework_version_immutability();

CREATE OR REPLACE FUNCTION enforce_requirement_version_immutability()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP = 'DELETE' THEN
        RAISE EXCEPTION 'Cannot delete requirement version %.', OLD.id;
    END IF;

    -- Finalize the semantic digest immediately after insert, or attach the
    -- one-time hierarchy parent link after a two-pass fixture/import insert.
    IF OLD.semantic_digest = 'pending'
       AND (to_jsonb(NEW) - 'semantic_digest')
           IS DISTINCT FROM (to_jsonb(OLD) - 'semantic_digest') THEN
        RAISE EXCEPTION 'Cannot modify requirement version % while finalizing its digest.', OLD.id;
    END IF;
    IF OLD.semantic_digest <> 'pending'
       AND OLD.parent_requirement_version_id IS NULL
       AND NEW.parent_requirement_version_id IS NOT NULL
       AND (to_jsonb(NEW) - 'parent_requirement_version_id')
           IS NOT DISTINCT FROM (to_jsonb(OLD) - 'parent_requirement_version_id') THEN
        RETURN NEW;
    END IF;
    IF OLD.semantic_digest <> 'pending' THEN
        RAISE EXCEPTION 'Cannot modify immutable requirement version %.', OLD.id;
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER compliance_requirement_versions_immutability
BEFORE UPDATE OR DELETE ON compliance_requirement_versions
FOR EACH ROW EXECUTE FUNCTION enforce_requirement_version_immutability();

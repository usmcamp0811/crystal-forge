-- Restrict the temporary v4 legacy reconstruction exception introduced by
-- 0220. Structural identity fields remain immutable even while the parent
-- framework version is pending.

CREATE OR REPLACE FUNCTION enforce_requirement_version_immutability()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
DECLARE
    framework_pending boolean;
BEGIN
    IF TG_OP = 'DELETE' THEN
        RAISE EXCEPTION 'Cannot delete requirement version %.', OLD.id;
    END IF;

    SELECT semantic_digest = 'pending'
    INTO framework_pending
    FROM compliance_framework_versions
    WHERE id = OLD.framework_version_id;

    IF OLD.semantic_digest = 'pending' AND framework_pending THEN
        IF (to_jsonb(NEW)
              - 'external_id' - 'title' - 'description' - 'kind'
              - 'parent_requirement_version_id' - 'severity' - 'check_text'
              - 'fix_text' - 'metadata' - 'semantic_digest'
              - 'digest_algorithm' - 'canonicalization_version')
           IS DISTINCT FROM
           (to_jsonb(OLD)
              - 'external_id' - 'title' - 'description' - 'kind'
              - 'parent_requirement_version_id' - 'severity' - 'check_text'
              - 'fix_text' - 'metadata' - 'semantic_digest'
              - 'digest_algorithm' - 'canonicalization_version') THEN
            RAISE EXCEPTION 'Cannot modify structural identity of requirement version % during v4 reconstruction.', OLD.id;
        END IF;
        RETURN NEW;
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

-- cf-model-json-4 includes structural group content in addition to hierarchy
-- edges. Re-open every v3 row so the application can reconstruct legacy STIG
-- topology from its persisted source artifact before finalizing the digest.

DROP TRIGGER compliance_framework_versions_immutability
    ON compliance_framework_versions;
DROP TRIGGER compliance_requirement_versions_immutability
    ON compliance_requirement_versions;

UPDATE compliance_framework_versions
SET semantic_digest = 'pending',
    canonicalization_version = 'cf-model-json-4'
WHERE canonicalization_version = 'cf-model-json-3';

UPDATE compliance_requirement_versions rv
SET semantic_digest = 'pending'
FROM compliance_framework_versions fv
WHERE fv.id = rv.framework_version_id
  AND fv.semantic_digest = 'pending';

CREATE OR REPLACE FUNCTION enforce_framework_version_immutability()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP = 'DELETE' THEN
        RAISE EXCEPTION 'Cannot delete framework version %.', OLD.id;
    END IF;

    IF OLD.semantic_digest = 'pending' THEN
        IF (to_jsonb(NEW) - 'semantic_digest' - 'digest_algorithm'
            - 'canonicalization_version')
           IS DISTINCT FROM
           (to_jsonb(OLD) - 'semantic_digest' - 'digest_algorithm'
            - 'canonicalization_version') THEN
            RAISE EXCEPTION 'Cannot modify framework version % during digest finalization.', OLD.id;
        END IF;
        RETURN NEW;
    END IF;

    RAISE EXCEPTION 'Cannot modify immutable framework version %.', OLD.id;
END;
$$;

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

    IF OLD.semantic_digest = 'pending' THEN
        -- A v4 legacy upgrade may need to replace the pending row's old
        -- canonical content before its parent link and digest are finalized.
        IF framework_pending THEN
            RETURN NEW;
        END IF;
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

CREATE TRIGGER compliance_framework_versions_immutability
BEFORE UPDATE OR DELETE ON compliance_framework_versions
FOR EACH ROW EXECUTE FUNCTION enforce_framework_version_immutability();

CREATE TRIGGER compliance_requirement_versions_immutability
BEFORE UPDATE OR DELETE ON compliance_requirement_versions
FOR EACH ROW EXECUTE FUNCTION enforce_requirement_version_immutability();

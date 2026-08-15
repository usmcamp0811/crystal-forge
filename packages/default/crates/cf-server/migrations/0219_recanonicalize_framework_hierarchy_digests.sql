-- Framework release identity now includes a deterministic hierarchy-edge
-- projection. Re-open existing rows so startup backfill can recompute them
-- with cf-model-json-3 without mutating immutable content fields.

DROP TRIGGER compliance_framework_versions_immutability
    ON compliance_framework_versions;

UPDATE compliance_framework_versions
SET semantic_digest = 'pending',
    canonicalization_version = 'cf-model-json-3'
WHERE canonicalization_version <> 'cf-model-json-3'
   OR semantic_digest = 'pending';

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

CREATE TRIGGER compliance_framework_versions_immutability
BEFORE UPDATE OR DELETE ON compliance_framework_versions
FOR EACH ROW EXECUTE FUNCTION enforce_framework_version_immutability();

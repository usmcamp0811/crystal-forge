-- Framework release identity now includes the complete normalized requirement
-- digest set. Re-open old rows for one controlled application backfill before
-- the immutable trigger is restored.

DROP TRIGGER compliance_framework_versions_immutability
    ON compliance_framework_versions;

UPDATE compliance_framework_versions
SET semantic_digest = 'pending'
WHERE canonicalization_version = 'cf-model-json-1';

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

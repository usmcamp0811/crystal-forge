-- Publisher is part of framework release semantic identity, so store an
-- immutable snapshot on each release rather than deriving it from mutable
-- lineage metadata. Recanonicalize v4 releases under cf-model-json-5.

ALTER TABLE compliance_framework_versions
    ADD COLUMN publisher TEXT NOT NULL DEFAULT '';

UPDATE compliance_framework_versions fv
SET publisher = COALESCE(f.publisher, '')
FROM compliance_frameworks f
WHERE f.id = fv.framework_id;

DROP TRIGGER compliance_framework_versions_immutability
    ON compliance_framework_versions;

UPDATE compliance_framework_versions
SET semantic_digest = 'pending',
    canonicalization_version = 'cf-model-json-5'
WHERE canonicalization_version = 'cf-model-json-4';

CREATE OR REPLACE FUNCTION enforce_framework_version_immutability()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP = 'DELETE' THEN
        RAISE EXCEPTION 'Cannot delete framework version %.', OLD.id;
    END IF;
    IF OLD.semantic_digest = 'pending' THEN
        IF (to_jsonb(NEW) - 'semantic_digest' - 'digest_algorithm'
            - 'canonicalization_version' - 'version' - 'title' - 'publisher')
           IS DISTINCT FROM
           (to_jsonb(OLD) - 'semantic_digest' - 'digest_algorithm'
            - 'canonicalization_version' - 'version' - 'title' - 'publisher') THEN
            RAISE EXCEPTION 'Cannot modify framework version % during v5 reconstruction.', OLD.id;
        END IF;
        RETURN NEW;
    END IF;
    RAISE EXCEPTION 'Cannot modify immutable framework version %.', OLD.id;
END;
$$;

CREATE TRIGGER compliance_framework_versions_immutability
BEFORE UPDATE OR DELETE ON compliance_framework_versions
FOR EACH ROW EXECUTE FUNCTION enforce_framework_version_immutability();

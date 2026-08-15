-- Post-0223 framework releases whose prior identity cannot be reconstructed
-- are retained as explicitly unresolved rather than blocking server startup.

ALTER TABLE compliance_framework_versions
    ADD COLUMN migration_recovery_status TEXT NOT NULL DEFAULT 'finalized',
    ADD COLUMN migration_recovery_reason TEXT;

ALTER TABLE compliance_framework_versions
    ADD CONSTRAINT compliance_framework_versions_recovery_status_check
    CHECK (migration_recovery_status IN ('finalized', 'pending', 'unresolved'));

UPDATE compliance_framework_versions
SET migration_recovery_status = 'pending'
WHERE semantic_digest = 'pending'
  AND canonicalization_version = 'cf-model-json-5';

CREATE OR REPLACE FUNCTION enforce_framework_version_immutability()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP = 'DELETE' THEN
        RAISE EXCEPTION 'Cannot delete framework version %.', OLD.id;
    END IF;
    IF OLD.semantic_digest = 'pending' THEN
        IF (to_jsonb(NEW) - 'semantic_digest' - 'digest_algorithm'
            - 'canonicalization_version' - 'version' - 'title' - 'publisher'
            - 'migration_recovery_status' - 'migration_recovery_reason')
           IS DISTINCT FROM
           (to_jsonb(OLD) - 'semantic_digest' - 'digest_algorithm'
            - 'canonicalization_version' - 'version' - 'title' - 'publisher'
            - 'migration_recovery_status' - 'migration_recovery_reason') THEN
            RAISE EXCEPTION 'Cannot modify framework version % during recovery.', OLD.id;
        END IF;
        RETURN NEW;
    END IF;
    RAISE EXCEPTION 'Cannot modify immutable framework version %.', OLD.id;
END;
$$;

-- An unresolved pending release may be retried only after an operator attaches
-- an authoritative source artifact. All other framework-version fields remain
-- immutable during recovery.

CREATE OR REPLACE FUNCTION enforce_framework_version_immutability()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP = 'DELETE' THEN
        RAISE EXCEPTION 'Cannot delete framework version %.', OLD.id;
    END IF;
    IF OLD.semantic_digest = 'pending' THEN
        IF NEW.source_artifact_id IS DISTINCT FROM OLD.source_artifact_id
           AND NOT (OLD.migration_recovery_status = 'unresolved'
                    AND OLD.source_artifact_id IS NULL
                    AND NEW.source_artifact_id IS NOT NULL) THEN
            RAISE EXCEPTION 'Cannot replace framework version % source artifact during recovery.', OLD.id;
        END IF;
        IF (to_jsonb(NEW) - 'semantic_digest' - 'digest_algorithm'
            - 'canonicalization_version' - 'version' - 'title' - 'publisher'
            - 'migration_recovery_status' - 'migration_recovery_reason'
            - 'source_artifact_id')
           IS DISTINCT FROM
           (to_jsonb(OLD) - 'semantic_digest' - 'digest_algorithm'
            - 'canonicalization_version' - 'version' - 'title' - 'publisher'
            - 'migration_recovery_status' - 'migration_recovery_reason'
            - 'source_artifact_id') THEN
            RAISE EXCEPTION 'Cannot modify framework version % during recovery.', OLD.id;
        END IF;
        RETURN NEW;
    END IF;
    RAISE EXCEPTION 'Cannot modify immutable framework version %.', OLD.id;
END;
$$;

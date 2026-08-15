-- Narrows the framework-version immutability trigger to allow exactly one
-- additional transition: an unresolved pending release with no source artifact
-- may have its artifact attached while simultaneously moving to pending status.
--
-- Permitted transition:
--   OLD: digest='pending', status='unresolved', source_artifact_id=NULL
--   NEW: digest='pending', status='pending',    source_artifact_id=<uuid>,
--        recovery_reason=NULL, all other fields identical.
--
-- All other mutations remain forbidden (finalized rows or mismatched fields
-- still raise an exception).

CREATE OR REPLACE FUNCTION enforce_framework_version_immutability()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP = 'DELETE' THEN
        RAISE EXCEPTION 'Cannot delete framework version %.', OLD.id;
    END IF;
    IF OLD.semantic_digest = 'pending' THEN
        -- Allow the one-time artifact-attachment recovery transition.
        IF OLD.migration_recovery_status = 'unresolved'
           AND OLD.source_artifact_id IS NULL
           AND NEW.source_artifact_id IS NOT NULL
           AND NEW.migration_recovery_status = 'pending'
           AND NEW.migration_recovery_reason IS NULL
           AND NEW.semantic_digest = 'pending' THEN
            -- Verify no structural identity fields changed.
            IF (to_jsonb(NEW)
                  - 'migration_recovery_status' - 'migration_recovery_reason'
                  - 'source_artifact_id')
               IS DISTINCT FROM
               (to_jsonb(OLD)
                  - 'migration_recovery_status' - 'migration_recovery_reason'
                  - 'source_artifact_id') THEN
                RAISE EXCEPTION
                    'Cannot modify framework version % structural fields during artifact attachment.', OLD.id;
            END IF;
            RETURN NEW;
        END IF;
        -- Allow the general recovery-state mutation path (digest finalization,
        -- metadata repair, unresolved marking) that was already permitted.
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

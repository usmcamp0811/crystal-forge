-- A v4 DISA reconstruction replaces digest-bearing release metadata from the
-- authoritative source artifact before finalizing the pending release digest.

CREATE OR REPLACE FUNCTION enforce_framework_version_immutability()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP = 'DELETE' THEN
        RAISE EXCEPTION 'Cannot delete framework version %.', OLD.id;
    END IF;

    IF OLD.semantic_digest = 'pending' THEN
        IF (to_jsonb(NEW) - 'semantic_digest' - 'digest_algorithm'
            - 'canonicalization_version' - 'version' - 'title')
           IS DISTINCT FROM
           (to_jsonb(OLD) - 'semantic_digest' - 'digest_algorithm'
            - 'canonicalization_version' - 'version' - 'title') THEN
            RAISE EXCEPTION 'Cannot modify framework version % during v4 reconstruction.', OLD.id;
        END IF;
        RETURN NEW;
    END IF;

    RAISE EXCEPTION 'Cannot modify immutable framework version %.', OLD.id;
END;
$$;

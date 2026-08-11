-- Migration 0205: allow one-time repair of Rust-authoritative semantic digests.
--
-- Migration 0200 replaced SQL digest values with the `pending` sentinel. Some
-- installations already had immutable versions when that reset ran. Those
-- rows must still be repaired at startup, but ordinary semantic edits to an
-- accepted or deprecated version must remain forbidden.

CREATE OR REPLACE FUNCTION enforce_policy_version_immutability()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    IF OLD.publication_state IN ('accepted', 'deprecated') THEN
        IF TG_OP = 'UPDATE'
           AND OLD.semantic_digest = 'pending'
           AND NEW.semantic_digest <> 'pending'
           AND (to_jsonb(NEW) - 'semantic_digest' - 'digest_algorithm'
                - 'canonicalization_version')
               IS NOT DISTINCT FROM
               (to_jsonb(OLD) - 'semantic_digest' - 'digest_algorithm'
                - 'canonicalization_version')
        THEN
            RETURN NEW;
        END IF;
        IF OLD.publication_state = 'accepted'
           AND TG_OP = 'UPDATE'
           AND NEW.publication_state = 'deprecated'
           AND (to_jsonb(NEW) - 'publication_state')
               IS NOT DISTINCT FROM (to_jsonb(OLD) - 'publication_state')
        THEN
            RETURN NEW;
        END IF;
        IF TG_OP = 'DELETE' THEN
            RAISE EXCEPTION 'Cannot delete % policy version %.', OLD.publication_state, OLD.id;
        END IF;
        RAISE EXCEPTION 'Cannot modify % policy version %.', OLD.publication_state, OLD.id;
    END IF;
    IF TG_OP = 'DELETE' THEN
        RETURN OLD;
    END IF;
    RETURN NEW;
END;
$$;

CREATE OR REPLACE FUNCTION enforce_bundle_version_immutability()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    IF OLD.publication_state IN ('accepted', 'deprecated') THEN
        IF TG_OP = 'UPDATE'
           AND OLD.semantic_digest = 'pending'
           AND NEW.semantic_digest <> 'pending'
           AND (to_jsonb(NEW) - 'semantic_digest' - 'digest_algorithm'
                - 'canonicalization_version')
               IS NOT DISTINCT FROM
               (to_jsonb(OLD) - 'semantic_digest' - 'digest_algorithm'
                - 'canonicalization_version')
        THEN
            RETURN NEW;
        END IF;
        IF OLD.publication_state = 'accepted'
           AND TG_OP = 'UPDATE'
           AND NEW.publication_state = 'deprecated'
           AND (to_jsonb(NEW) - 'publication_state')
               IS NOT DISTINCT FROM (to_jsonb(OLD) - 'publication_state')
        THEN
            RETURN NEW;
        END IF;
        IF TG_OP = 'DELETE' THEN
            RAISE EXCEPTION 'Cannot delete % bundle version %.', OLD.publication_state, OLD.id;
        END IF;
        RAISE EXCEPTION 'Cannot modify % bundle version %.', OLD.publication_state, OLD.id;
    END IF;
    IF TG_OP = 'DELETE' THEN
        RETURN OLD;
    END IF;
    RETURN NEW;
END;
$$;

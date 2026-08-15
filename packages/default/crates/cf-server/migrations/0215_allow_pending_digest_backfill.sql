-- Digest columns were added with a 'pending' sentinel for existing rows.
-- Permit the startup backfill to populate only those columns on immutable
-- versions without weakening any other immutability rule.

CREATE OR REPLACE FUNCTION enforce_bundle_version_immutability()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    IF OLD.publication_state IN ('accepted', 'deprecated')
       AND OLD.requirement_digest = 'pending' THEN
        IF TG_OP = 'UPDATE'
           AND (to_jsonb(NEW) - 'requirement_digest')
               IS DISTINCT FROM (to_jsonb(OLD) - 'requirement_digest')
        THEN
            RAISE EXCEPTION 'Digest backfill changed non-digest fields for bundle version %.', OLD.id;
        END IF;
        IF TG_OP = 'DELETE' THEN
            RAISE EXCEPTION 'Cannot delete immutable bundle version %.', OLD.id;
        END IF;
        RETURN NEW;
    END IF;

    IF OLD.publication_state = 'accepted' THEN
        IF TG_OP = 'DELETE' THEN
            RAISE EXCEPTION 'Cannot delete accepted bundle version %.', OLD.id;
        END IF;
        IF NEW.publication_state = 'deprecated' THEN
            IF (to_jsonb(NEW) - 'publication_state')
               IS DISTINCT FROM (to_jsonb(OLD) - 'publication_state')
            THEN
                RAISE EXCEPTION
                    'Deprecating accepted bundle version % but non-lifecycle fields changed.', OLD.id;
            END IF;
            RETURN NEW;
        END IF;
        RAISE EXCEPTION 'Cannot modify accepted bundle version %.', OLD.id;
    END IF;
    IF OLD.publication_state = 'deprecated' THEN
        RAISE EXCEPTION 'Cannot modify deprecated bundle version %.', OLD.id;
    END IF;
    IF TG_OP = 'DELETE' THEN RETURN OLD; END IF;
    RETURN NEW;
END;
$$;

CREATE OR REPLACE FUNCTION enforce_policy_version_immutability()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    IF OLD.publication_state IN ('accepted', 'deprecated')
       AND OLD.mapping_digest = 'pending' THEN
        IF TG_OP = 'UPDATE'
           AND (to_jsonb(NEW) - 'mapping_digest')
               IS DISTINCT FROM (to_jsonb(OLD) - 'mapping_digest')
        THEN
            RAISE EXCEPTION 'Digest backfill changed non-digest fields for policy version %.', OLD.id;
        END IF;
        IF TG_OP = 'DELETE' THEN
            RAISE EXCEPTION 'Cannot delete immutable policy version %.', OLD.id;
        END IF;
        RETURN NEW;
    END IF;

    IF OLD.publication_state = 'accepted' THEN
        IF TG_OP = 'DELETE' THEN
            RAISE EXCEPTION 'Cannot delete accepted policy version %.', OLD.id;
        END IF;
        IF NEW.publication_state = 'deprecated' THEN
            IF (to_jsonb(NEW) - 'publication_state')
               IS DISTINCT FROM (to_jsonb(OLD) - 'publication_state')
            THEN
                RAISE EXCEPTION
                    'Deprecating accepted policy version % but non-lifecycle fields changed.', OLD.id;
            END IF;
            RETURN NEW;
        END IF;
        RAISE EXCEPTION 'Cannot modify accepted policy version %.', OLD.id;
    END IF;
    IF OLD.publication_state = 'deprecated' THEN
        RAISE EXCEPTION 'Cannot modify deprecated policy version %.', OLD.id;
    END IF;
    IF TG_OP = 'DELETE' THEN RETURN OLD; END IF;
    RETURN NEW;
END;
$$;

-- Migration 0226: allow cleanup of assignment history belonging only to
-- disposable bundle versions while retaining accepted/deprecated history.

CREATE OR REPLACE FUNCTION prevent_assignment_version_mutation()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP = 'DELETE' THEN
        IF EXISTS (
            SELECT 1
            FROM compliance_bundle_versions bv
            WHERE bv.id = OLD.bundle_version_id
              AND bv.publication_state IN ('accepted', 'deprecated')
        ) THEN
            RAISE EXCEPTION
                'Assignment version % is immutable because it references immutable bundle history.',
                OLD.id;
        END IF;
        RETURN OLD;
    END IF;

    RAISE EXCEPTION 'Assignment versions are immutable';
END;
$$;

CREATE OR REPLACE FUNCTION prevent_assignment_child_mutation()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP = 'DELETE' THEN
        IF EXISTS (
            SELECT 1
            FROM compliance_bundle_assignment_versions av
            JOIN compliance_bundle_versions bv ON bv.id = av.bundle_version_id
            WHERE av.id = OLD.assignment_version_id
              AND bv.publication_state IN ('accepted', 'deprecated')
        ) THEN
            RAISE EXCEPTION
                'Assignment version child is immutable because it references immutable bundle history.';
        END IF;
        RETURN OLD;
    END IF;

    IF TG_OP <> 'INSERT' THEN
        RAISE EXCEPTION 'Assignment version children are immutable';
    END IF;
    RETURN NEW;
END;
$$;

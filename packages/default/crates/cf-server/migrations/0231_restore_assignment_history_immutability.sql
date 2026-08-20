-- Assignment snapshots and their children are immutable audit history,
-- regardless of the publication state of the referenced bundle version.

CREATE OR REPLACE FUNCTION prevent_assignment_version_mutation()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    RAISE EXCEPTION 'Assignment versions are immutable';
END;
$$;

CREATE OR REPLACE FUNCTION prevent_assignment_child_mutation()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP <> 'INSERT' THEN
        RAISE EXCEPTION 'Assignment version children are immutable';
    END IF;
    RETURN NEW;
END;
$$;

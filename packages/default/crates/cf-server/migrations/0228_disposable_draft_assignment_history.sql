-- Draft-only assignment history is disposable with its draft bundle lineage.
-- Published/deprecated assignment history remains immutable for auditability.

CREATE OR REPLACE FUNCTION prevent_assignment_version_mutation()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP = 'DELETE'
       AND EXISTS (
           SELECT 1
           FROM compliance_bundle_versions bv
           WHERE bv.id = OLD.bundle_version_id
             AND bv.publication_state IN ('incomplete', 'draft', 'interim')
       ) THEN
        RETURN OLD;
    END IF;

    RAISE EXCEPTION 'Assignment versions are immutable';
END;
$$;

CREATE OR REPLACE FUNCTION prevent_assignment_child_mutation()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP = 'INSERT' THEN
        RETURN NEW;
    END IF;

    IF TG_OP = 'DELETE'
       AND EXISTS (
           SELECT 1
           FROM compliance_bundle_assignment_versions av
           JOIN compliance_bundle_versions bv ON bv.id = av.bundle_version_id
           WHERE av.id = OLD.assignment_version_id
             AND bv.publication_state IN ('incomplete', 'draft', 'interim')
       ) THEN
        RETURN OLD;
    END IF;

    RAISE EXCEPTION 'Assignment version children are immutable';
END;
$$;

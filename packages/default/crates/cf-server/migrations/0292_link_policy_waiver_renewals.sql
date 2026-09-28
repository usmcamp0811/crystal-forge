-- A review deadline is not an authorization expiry. The predecessor revision
-- records the exact source version so a retry cannot renew a later decision.
ALTER TABLE finding_waivers
    ADD COLUMN review_due_at date,
    ADD COLUMN predecessor_id uuid REFERENCES finding_waivers(id) ON DELETE RESTRICT,
    ADD COLUMN predecessor_updated_at timestamptz,
    ADD CONSTRAINT finding_waiver_successor_pair CHECK
        ((predecessor_id IS NULL) = (predecessor_updated_at IS NULL)),
    ADD CONSTRAINT finding_waiver_successor_review CHECK
        (predecessor_id IS NULL OR review_due_at IS NOT NULL),
    ADD CONSTRAINT finding_waiver_no_self_link CHECK
        (predecessor_id IS NULL OR predecessor_id <> id);

CREATE UNIQUE INDEX finding_waivers_one_successor
    ON finding_waivers(predecessor_id) WHERE predecessor_id IS NOT NULL;

CREATE OR REPLACE FUNCTION protect_finding_waiver_renewal()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF NEW.review_due_at IS DISTINCT FROM OLD.review_due_at
       OR NEW.predecessor_id IS DISTINCT FROM OLD.predecessor_id
       OR NEW.predecessor_updated_at IS DISTINCT FROM OLD.predecessor_updated_at THEN
        RAISE EXCEPTION 'Finding waiver renewal provenance is immutable'
            USING ERRCODE='23514', CONSTRAINT='finding_waiver_renewal_immutable';
    END IF;
    RETURN NEW;
END;
$$;
CREATE TRIGGER trigger_protect_finding_waiver_renewal
    BEFORE UPDATE ON finding_waivers
    FOR EACH ROW EXECUTE FUNCTION protect_finding_waiver_renewal();

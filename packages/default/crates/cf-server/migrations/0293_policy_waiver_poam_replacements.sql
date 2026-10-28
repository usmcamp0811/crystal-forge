-- One immutable, source-versioned replacement per selected waiver. A plan
-- remains a policy-family plan with a link to the source's stable finding.
ALTER TABLE poam_activity DROP CONSTRAINT poam_activity_kind_check;
ALTER TABLE poam_activity ADD CONSTRAINT poam_activity_kind_check CHECK (kind IN (
    'created', 'updated', 'status_changed', 'milestone_added',
    'milestone_updated', 'milestone_removed', 'note', 'finding_linked',
    'finding_unlinked', 'assignment_linked', 'assignment_unlinked',
    'verification_attempted', 'closed', 'reopened', 'waiver_converted'
));

CREATE TABLE finding_waiver_poam_replacements (
    waiver_id uuid PRIMARY KEY REFERENCES finding_waivers(id) ON DELETE RESTRICT,
    source_updated_at timestamptz NOT NULL,
    poam_id uuid NOT NULL REFERENCES poams(id) ON DELETE RESTRICT,
    finding_id uuid NOT NULL,
    converted_by uuid NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    converted_at timestamptz NOT NULL,
    request_digest text NOT NULL CHECK (btrim(request_digest) <> ''),
    reused boolean NOT NULL,
    FOREIGN KEY (waiver_id, finding_id)
        REFERENCES finding_waivers(id, finding_id) ON DELETE RESTRICT
);

CREATE FUNCTION guard_finding_waiver_poam_replacement()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP <> 'INSERT' THEN
        RAISE EXCEPTION 'Finding waiver replacement history is immutable'
            USING ERRCODE='23514', CONSTRAINT='finding_waiver_replacement_immutable';
    END IF;
    IF NOT EXISTS (
        SELECT 1 FROM finding_waivers w
        WHERE w.id=NEW.waiver_id AND w.finding_id=NEW.finding_id
          AND w.status IN ('revoked','expired')
    ) OR NOT EXISTS (
        SELECT 1 FROM poam_finding_links link
        WHERE link.poam_id=NEW.poam_id AND link.finding_id=NEW.finding_id
          AND link.retired_at IS NULL
    ) OR EXISTS (
        SELECT 1 FROM poam_cve_finding_links link
        WHERE link.poam_id=NEW.poam_id AND link.retired_at IS NULL
    ) THEN
        RAISE EXCEPTION 'Replacement must bind a revoked waiver to its active policy remediation'
            USING ERRCODE='23514', CONSTRAINT='finding_waiver_replacement_context';
    END IF;
    RETURN NEW;
END;
$$;
CREATE TRIGGER trigger_guard_finding_waiver_poam_replacement
    BEFORE INSERT OR UPDATE OR DELETE ON finding_waiver_poam_replacements
    FOR EACH ROW EXECUTE FUNCTION guard_finding_waiver_poam_replacement();

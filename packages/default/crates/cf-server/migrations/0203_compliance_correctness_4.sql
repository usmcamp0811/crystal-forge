-- Migration 0203: Compliance foundation correctness – round 8.
--
-- 1. Fix invalid trigger WHEN clauses from 0202 (P1 #1):
--    Separate INSERT and UPDATE triggers; UPDATE WHEN clause uses only OLD/NEW
--    without TG_OP.
--
-- 2. Add trigger that validates lineage pointers when a version's
--    publication_state changes (P1 #5). Uses a DEFERRED constraint trigger so
--    both the version and lineage pointer can be updated in the same transaction.

-- ── Drop and recreate the 0202 triggers with correct syntax ──────────────────

DROP TRIGGER IF EXISTS trigger_policy_version_pointer_integrity
    ON deployment_policies;
DROP TRIGGER IF EXISTS trigger_bundle_version_pointer_integrity
    ON compliance_bundles;

-- Policy: separate INSERT and UPDATE triggers.
CREATE TRIGGER trigger_policy_version_pointer_integrity_insert
    BEFORE INSERT ON deployment_policies
    FOR EACH ROW
    EXECUTE FUNCTION enforce_policy_version_pointer_integrity();

CREATE TRIGGER trigger_policy_version_pointer_integrity_update
    BEFORE UPDATE OF current_draft_version_id, current_published_version_id
    ON deployment_policies
    FOR EACH ROW
    WHEN (
        OLD.current_draft_version_id IS DISTINCT FROM NEW.current_draft_version_id
     OR OLD.current_published_version_id IS DISTINCT FROM NEW.current_published_version_id
    )
    EXECUTE FUNCTION enforce_policy_version_pointer_integrity();

-- Bundle: separate INSERT and UPDATE triggers.
CREATE TRIGGER trigger_bundle_version_pointer_integrity_insert
    BEFORE INSERT ON compliance_bundles
    FOR EACH ROW
    EXECUTE FUNCTION enforce_bundle_version_pointer_integrity();

CREATE TRIGGER trigger_bundle_version_pointer_integrity_update
    BEFORE UPDATE OF current_draft_version_id, current_published_version_id
    ON compliance_bundles
    FOR EACH ROW
    WHEN (
        OLD.current_draft_version_id IS DISTINCT FROM NEW.current_draft_version_id
     OR OLD.current_published_version_id IS DISTINCT FROM NEW.current_published_version_id
    )
    EXECUTE FUNCTION enforce_bundle_version_pointer_integrity();

-- ── P1 #5: Validate lineage pointers when a version changes state ─────────────
-- Uses a DEFERRED constraint trigger so the lineage pointer update and the
-- version state change can occur in the same transaction in any order.

CREATE OR REPLACE FUNCTION validate_policy_lineage_pointer_after_state_change()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    -- After any publication_state change, verify the owning policy's pointers.
    -- Accepted/deprecated → current_draft pointer must NOT point here.
    IF NEW.publication_state IN ('accepted', 'deprecated') THEN
        IF EXISTS (
            SELECT 1 FROM deployment_policies
            WHERE id = NEW.policy_id
              AND current_draft_version_id = NEW.id
        ) THEN
            RAISE EXCEPTION
                'Policy version % moved to immutable state % but is still referenced '
                'as current_draft_version_id on policy %. '
                'Update current_draft_version_id first.',
                NEW.id, NEW.publication_state, NEW.policy_id;
        END IF;
    END IF;
    -- Draft/incomplete/interim → current_published pointer must NOT point here.
    IF NEW.publication_state IN ('incomplete', 'draft', 'interim') THEN
        IF EXISTS (
            SELECT 1 FROM deployment_policies
            WHERE id = NEW.policy_id
              AND current_published_version_id = NEW.id
        ) THEN
            RAISE EXCEPTION
                'Policy version % is in mutable state % but is still referenced '
                'as current_published_version_id on policy %. '
                'Update current_published_version_id first.',
                NEW.id, NEW.publication_state, NEW.policy_id;
        END IF;
    END IF;
    RETURN NEW;
END;
$$;

CREATE CONSTRAINT TRIGGER trigger_validate_policy_lineage_on_state_change
    AFTER UPDATE OF publication_state ON deployment_policy_versions
    DEFERRABLE INITIALLY DEFERRED
    FOR EACH ROW
    WHEN (OLD.publication_state IS DISTINCT FROM NEW.publication_state)
    EXECUTE FUNCTION validate_policy_lineage_pointer_after_state_change();

CREATE OR REPLACE FUNCTION validate_bundle_lineage_pointer_after_state_change()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    IF NEW.publication_state IN ('accepted', 'deprecated') THEN
        IF EXISTS (
            SELECT 1 FROM compliance_bundles
            WHERE id = NEW.bundle_id
              AND current_draft_version_id = NEW.id
        ) THEN
            RAISE EXCEPTION
                'Bundle version % moved to immutable state % but is still referenced '
                'as current_draft_version_id on bundle %.',
                NEW.id, NEW.publication_state, NEW.bundle_id;
        END IF;
    END IF;
    IF NEW.publication_state IN ('incomplete', 'draft', 'interim') THEN
        IF EXISTS (
            SELECT 1 FROM compliance_bundles
            WHERE id = NEW.bundle_id
              AND current_published_version_id = NEW.id
        ) THEN
            RAISE EXCEPTION
                'Bundle version % is in mutable state % but is still referenced '
                'as current_published_version_id on bundle %.',
                NEW.id, NEW.publication_state, NEW.bundle_id;
        END IF;
    END IF;
    RETURN NEW;
END;
$$;

CREATE CONSTRAINT TRIGGER trigger_validate_bundle_lineage_on_state_change
    AFTER UPDATE OF publication_state ON compliance_bundle_versions
    DEFERRABLE INITIALLY DEFERRED
    FOR EACH ROW
    WHEN (OLD.publication_state IS DISTINCT FROM NEW.publication_state)
    EXECUTE FUNCTION validate_bundle_lineage_pointer_after_state_change();

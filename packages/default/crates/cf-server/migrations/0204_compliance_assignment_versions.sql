-- Migration 0204: immutable assignment versions and optimistic concurrency.
--
-- compliance_bundle_assignments remains the assignment lineage/identity row.
-- Its current_version_id points at an immutable snapshot. Overlay rows are
-- keyed by that snapshot, so updates never delete or rewrite historical state.

CREATE TABLE compliance_bundle_assignment_versions (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    assignment_id uuid NOT NULL REFERENCES compliance_bundle_assignments(id) ON DELETE CASCADE,
    previous_version_id uuid REFERENCES compliance_bundle_assignment_versions(id) ON DELETE RESTRICT,
    version_number bigint NOT NULL,
    bundle_version_id uuid NOT NULL REFERENCES compliance_bundle_versions(id) ON DELETE RESTRICT,
    enforcement_mode text NOT NULL DEFAULT 'enforce',
    assignment_overlay_digest text NOT NULL,
    provenance jsonb NOT NULL DEFAULT '{}',
    created_by uuid REFERENCES users(id),
    created_at timestamptz NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE (assignment_id, version_number),
    CHECK (version_number > 0),
    CHECK (enforcement_mode IN ('enforce', 'report_only'))
);

ALTER TABLE compliance_bundle_assignments
    ADD COLUMN bundle_id uuid REFERENCES compliance_bundles(id) ON DELETE RESTRICT,
    ADD COLUMN current_version_id uuid,
    ADD COLUMN active boolean NOT NULL DEFAULT true;

UPDATE compliance_bundle_assignments a
SET bundle_id = bv.bundle_id
FROM compliance_bundle_versions bv
WHERE bv.id = a.bundle_version_id;

ALTER TABLE compliance_bundle_assignments
    ALTER COLUMN bundle_id SET NOT NULL;

CREATE UNIQUE INDEX compliance_bundle_assignments_environment_lineage_unique
    ON compliance_bundle_assignments (bundle_id, environment_id)
    WHERE environment_id IS NOT NULL AND active;
CREATE UNIQUE INDEX compliance_bundle_assignments_system_lineage_unique
    ON compliance_bundle_assignments (bundle_id, system_id)
    WHERE system_id IS NOT NULL AND active;

CREATE OR REPLACE FUNCTION sync_bundle_env_assignment_insert()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
DECLARE
    v_version_id uuid;
    v_bundle_id uuid;
BEGIN
    SELECT id, bundle_id INTO v_version_id, v_bundle_id
    FROM compliance_bundle_versions
    WHERE bundle_id = NEW.bundle_id
    ORDER BY (publication_state = 'draft') DESC, created_at DESC
    LIMIT 1;
    IF v_version_id IS NULL THEN
        RETURN NEW;
    END IF;

    INSERT INTO compliance_bundle_assignments (
        bundle_id, bundle_version_id, scope_type, environment_id,
        enforcement_mode, assignment_overlay_digest
    ) VALUES (
        v_bundle_id, v_version_id, 'environment', NEW.environment_id,
        'enforce', 'pending'
    )
    ON CONFLICT DO NOTHING;
    RETURN NEW;
END;
$$;

CREATE OR REPLACE FUNCTION sync_bundle_env_assignment_delete()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    UPDATE compliance_bundle_assignments
    SET active = false, current_version_id = NULL
    WHERE bundle_id = OLD.bundle_id
      AND scope_type = 'environment'
      AND environment_id = OLD.environment_id;
    RETURN OLD;
END;
$$;

ALTER TABLE compliance_assignment_exclusions
    ADD COLUMN assignment_version_id uuid REFERENCES compliance_bundle_assignment_versions(id) ON DELETE CASCADE;
ALTER TABLE compliance_assignment_additions
    ADD COLUMN assignment_version_id uuid REFERENCES compliance_bundle_assignment_versions(id) ON DELETE CASCADE;
ALTER TABLE compliance_assignment_value_overrides
    ADD COLUMN assignment_version_id uuid REFERENCES compliance_bundle_assignment_versions(id) ON DELETE CASCADE;

-- Existing rows are the initial immutable snapshot for each legacy assignment.
INSERT INTO compliance_bundle_assignment_versions (
    assignment_id, version_number, bundle_version_id, enforcement_mode,
    assignment_overlay_digest, provenance, created_by, created_at
)
SELECT id, 1, bundle_version_id, enforcement_mode, assignment_overlay_digest,
       provenance, created_by, created_at
FROM compliance_bundle_assignments;

UPDATE compliance_assignment_exclusions e
SET assignment_version_id = v.id
FROM compliance_bundle_assignment_versions v
WHERE v.assignment_id = e.assignment_id AND v.version_number = 1;
UPDATE compliance_assignment_additions a
SET assignment_version_id = v.id
FROM compliance_bundle_assignment_versions v
WHERE v.assignment_id = a.assignment_id AND v.version_number = 1;
UPDATE compliance_assignment_value_overrides o
SET assignment_version_id = v.id
FROM compliance_bundle_assignment_versions v
WHERE v.assignment_id = o.assignment_id AND v.version_number = 1;

ALTER TABLE compliance_assignment_exclusions
    ALTER COLUMN assignment_version_id SET NOT NULL;
ALTER TABLE compliance_assignment_additions
    ALTER COLUMN assignment_version_id SET NOT NULL;
ALTER TABLE compliance_assignment_value_overrides
    ALTER COLUMN assignment_version_id SET NOT NULL;

ALTER TABLE compliance_bundle_assignments
    ADD CONSTRAINT compliance_bundle_assignments_current_version_fk
        FOREIGN KEY (current_version_id)
        REFERENCES compliance_bundle_assignment_versions(id)
        DEFERRABLE INITIALLY DEFERRED;

UPDATE compliance_bundle_assignments a
SET current_version_id = v.id
FROM compliance_bundle_assignment_versions v
WHERE v.assignment_id = a.id AND v.version_number = 1;

ALTER TABLE compliance_assignment_exclusions
    DROP CONSTRAINT compliance_assignment_exclusions_pkey,
    ADD PRIMARY KEY (assignment_version_id, policy_version_id);
ALTER TABLE compliance_assignment_additions
    DROP CONSTRAINT compliance_assignment_additions_pkey,
    ADD PRIMARY KEY (assignment_version_id, policy_version_id);
ALTER TABLE compliance_assignment_value_overrides
    DROP CONSTRAINT compliance_assignment_value_o_assignment_id_policy_version__key,
    ADD CONSTRAINT compliance_assignment_value_overrides_version_path_key
        UNIQUE (assignment_version_id, policy_version_id, value_path);

CREATE OR REPLACE FUNCTION prevent_assignment_version_mutation()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    RAISE EXCEPTION 'Assignment versions are immutable';
END;
$$;

CREATE TRIGGER trigger_prevent_assignment_version_update
    BEFORE UPDATE ON compliance_bundle_assignment_versions
    FOR EACH ROW EXECUTE FUNCTION prevent_assignment_version_mutation();
CREATE TRIGGER trigger_prevent_assignment_version_delete
    BEFORE DELETE ON compliance_bundle_assignment_versions
    FOR EACH ROW EXECUTE FUNCTION prevent_assignment_version_mutation();

CREATE OR REPLACE FUNCTION prevent_assignment_child_mutation()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP <> 'INSERT' THEN
        RAISE EXCEPTION 'Assignment version children are immutable';
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER trigger_prevent_assignment_exclusion_mutation
    BEFORE UPDATE OR DELETE ON compliance_assignment_exclusions
    FOR EACH ROW EXECUTE FUNCTION prevent_assignment_child_mutation();
CREATE TRIGGER trigger_prevent_assignment_addition_mutation
    BEFORE UPDATE OR DELETE ON compliance_assignment_additions
    FOR EACH ROW EXECUTE FUNCTION prevent_assignment_child_mutation();
CREATE TRIGGER trigger_prevent_assignment_override_mutation
    BEFORE UPDATE OR DELETE ON compliance_assignment_value_overrides
    FOR EACH ROW EXECUTE FUNCTION prevent_assignment_child_mutation();

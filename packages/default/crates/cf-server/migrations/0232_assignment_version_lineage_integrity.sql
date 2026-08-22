-- Assignment snapshot pointers must stay within their assignment lineage.
--
-- A plain FK from assignments.current_version_id to assignment_versions.id
-- proves only that the version exists. Without the composite constraints
-- below, assignment A can point at assignment B's immutable snapshot and read
-- B's bundle, reason, enforcement mode, and overlays under A's scope/identity.
-- The same defect allowed previous_version_id to splice two histories.

ALTER TABLE compliance_bundle_assignment_versions
    ADD CONSTRAINT compliance_assignment_versions_id_assignment_unique
        UNIQUE (id, assignment_id);

ALTER TABLE compliance_bundle_assignments
    ADD CONSTRAINT compliance_bundle_assignments_current_version_lineage_fk
        FOREIGN KEY (current_version_id, id)
        REFERENCES compliance_bundle_assignment_versions (id, assignment_id)
        DEFERRABLE INITIALLY DEFERRED;

ALTER TABLE compliance_bundle_assignment_versions
    ADD CONSTRAINT compliance_assignment_versions_previous_lineage_fk
        FOREIGN KEY (previous_version_id, assignment_id)
        REFERENCES compliance_bundle_assignment_versions (id, assignment_id)
        DEFERRABLE INITIALLY DEFERRED;

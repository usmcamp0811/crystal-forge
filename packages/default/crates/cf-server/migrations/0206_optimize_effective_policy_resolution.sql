-- Migration 0206: indexes for effective-policy scope lookups.
--
-- The resolver reads active assignments by environment or system. Earlier
-- indexes led with bundle_id and therefore could not efficiently serve these
-- lookups when resolving every active system during evaluation/deployment.

CREATE INDEX IF NOT EXISTS compliance_bundle_assignments_active_environment
    ON compliance_bundle_assignments (environment_id, bundle_id, id)
    WHERE active
      AND current_version_id IS NOT NULL
      AND scope_type = 'environment';

CREATE INDEX IF NOT EXISTS compliance_bundle_assignments_active_system
    ON compliance_bundle_assignments (system_id, bundle_id, id)
    WHERE active
      AND current_version_id IS NOT NULL
      AND scope_type = 'system';

CREATE INDEX IF NOT EXISTS compliance_assignment_exclusions_version
    ON compliance_assignment_exclusions (assignment_version_id);

CREATE INDEX IF NOT EXISTS compliance_assignment_additions_version
    ON compliance_assignment_additions (assignment_version_id);

CREATE INDEX IF NOT EXISTS compliance_assignment_overrides_version
    ON compliance_assignment_value_overrides (assignment_version_id);

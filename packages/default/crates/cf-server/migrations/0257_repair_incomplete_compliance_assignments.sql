-- Migration 0257: remove the legacy assignment writer and repair incomplete
-- assignment lineages.
--
-- compliance_bundle_environments records bundle eligibility/display metadata.
-- Only an explicit assignment mutation may create an authoritative assignment
-- lineage and its immutable version.

DROP TRIGGER IF EXISTS trigger_sync_bundle_env_assignment_insert
    ON compliance_bundle_environments;
DROP TRIGGER IF EXISTS trigger_sync_bundle_env_assignment_delete
    ON compliance_bundle_environments;
DROP FUNCTION IF EXISTS sync_bundle_env_assignment_insert();
DROP FUNCTION IF EXISTS sync_bundle_env_assignment_delete();

-- The pre-lineage indexes include inactive rows and therefore prevent a valid
-- replacement after an incomplete lineage is deactivated. The active-lineage
-- indexes created by migration 0204 enforce the current uniqueness contract.
DROP INDEX IF EXISTS compliance_bundle_assignments_environment_unique;
DROP INDEX IF EXISTS compliance_bundle_assignments_system_unique;

-- INVARIANT: An active lineage is authoritative only when current_version_id
-- selects an immutable version that belongs to the same lineage. Preserve all
-- lineage and version history, but remove incomplete rows from active authority.
UPDATE compliance_bundle_assignments AS assignment
SET active = false,
    current_version_id = NULL
WHERE assignment.active
  AND NOT EXISTS (
      SELECT 1
      FROM compliance_bundle_assignment_versions AS version
      WHERE version.id = assignment.current_version_id
        AND version.assignment_id = assignment.id
  );

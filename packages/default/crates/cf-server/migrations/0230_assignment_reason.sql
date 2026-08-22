-- Add reason field to compliance_bundle_assignment_versions for storing
-- user-provided justification for why a system/environment is pinned to a
-- specific bundle version.
--
-- Semantics: reason is entered during assignment creation/update and persists
-- with the immutable assignment version. It is displayed to users explaining
-- why the pinned assignment was made (e.g. "vendor testing", "migration in
-- progress", "regression hold").
--
-- reason is optional (NULL if not provided during assignment creation).
-- Each immutable version may have a different reason.

ALTER TABLE compliance_bundle_assignment_versions
ADD COLUMN reason TEXT NULL;

-- Add index for lookups (if needed for filtering by reason, though unlikely)
-- Omitted for now; add if performance analysis shows benefit.

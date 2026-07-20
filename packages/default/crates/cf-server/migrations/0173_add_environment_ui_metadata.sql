-- New environment UI-metadata columns. Existing rows get conservative,
-- uniform defaults (not name-based inference) so that a pre-existing
-- environment named e.g. "production" or "lab" is never automatically
-- reclassified as production or given a non-default deployment/approval
-- policy based solely on an arbitrary, user-controlled label. Administrators
-- must explicitly opt an environment into `is_production`, a non-manual
-- `default_policy`, or `auto_sync = false` via the Environments UI/API.
ALTER TABLE environments
    ADD COLUMN IF NOT EXISTS default_policy TEXT NOT NULL DEFAULT 'manual',
    ADD COLUMN IF NOT EXISTS auto_sync BOOLEAN NOT NULL DEFAULT TRUE,
    ADD COLUMN IF NOT EXISTS requires_approval BOOLEAN NOT NULL DEFAULT FALSE,
    ADD COLUMN IF NOT EXISTS is_production BOOLEAN NOT NULL DEFAULT FALSE;

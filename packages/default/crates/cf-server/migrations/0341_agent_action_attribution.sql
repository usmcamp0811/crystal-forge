-- Agent action attribution (TASK-534.5).
--
-- A write made through an owner access token keeps the human owner as the
-- actor and also records the acting token. NULL means a browser session or
-- server action. The column is a reference only: the token name that the
-- Audit Log shows is a snapshot in `admin_audit_events.metadata`, so the name
-- survives token deletion.
--
-- ON DELETE SET NULL: tokens are deleted only by the cascade from their owner.
-- `poam_activity` and `poam_verification_attempts` reject UPDATE through their
-- immutability triggers, but every row with a token id also names the same
-- owner as `actor_user_id` or `attempted_by`, and those rows already block the
-- owner's deletion. This column therefore adds no new deletion failure.

ALTER TABLE admin_audit_events
    ADD COLUMN IF NOT EXISTS access_token_id uuid
        REFERENCES api_access_tokens (id) ON DELETE SET NULL;

ALTER TABLE poam_activity
    ADD COLUMN IF NOT EXISTS access_token_id uuid
        REFERENCES api_access_tokens (id) ON DELETE SET NULL;

ALTER TABLE poam_verification_attempts
    ADD COLUMN IF NOT EXISTS access_token_id uuid
        REFERENCES api_access_tokens (id) ON DELETE SET NULL;

-- Audit Log filtering by token. Most events have no token, so the index is
-- partial.
CREATE INDEX IF NOT EXISTS idx_admin_audit_events_access_token_id
    ON admin_audit_events (access_token_id)
    WHERE access_token_id IS NOT NULL;

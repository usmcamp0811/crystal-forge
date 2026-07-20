CREATE TABLE IF NOT EXISTS admin_audit_events (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    actor_user_id uuid REFERENCES users(id),
    actor_identifier text,
    action text NOT NULL,
    target text NOT NULL,
    request_origin text,
    metadata jsonb NOT NULL DEFAULT '{}'::jsonb,
    created_at timestamptz NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_admin_audit_events_created_at
    ON admin_audit_events (created_at DESC);

CREATE INDEX IF NOT EXISTS idx_admin_audit_events_action
    ON admin_audit_events (action);

CREATE INDEX IF NOT EXISTS idx_admin_audit_events_actor_user_id
    ON admin_audit_events (actor_user_id);

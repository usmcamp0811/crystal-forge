CREATE TABLE IF NOT EXISTS user_environment_memberships (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    environment_id uuid NOT NULL REFERENCES environments(id) ON DELETE CASCADE,
    assigned_by_user_id uuid REFERENCES users(id),
    created_at timestamptz NOT NULL DEFAULT NOW(),
    UNIQUE (user_id, environment_id)
);

CREATE INDEX IF NOT EXISTS idx_user_environment_memberships_user_id
    ON user_environment_memberships (user_id);

CREATE INDEX IF NOT EXISTS idx_user_environment_memberships_environment_id
    ON user_environment_memberships (environment_id);

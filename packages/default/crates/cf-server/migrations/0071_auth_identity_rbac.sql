CREATE TYPE auth_role AS ENUM (
    'admin',
    'operator',
    'viewer'
);

CREATE TABLE IF NOT EXISTS external_identities (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    provider_key varchar(80) NOT NULL,
    subject text NOT NULL,
    tenant_discriminator varchar(120) NOT NULL DEFAULT '',
    claims jsonb NOT NULL DEFAULT '{}'::jsonb,
    created_at timestamptz NOT NULL DEFAULT NOW(),
    updated_at timestamptz NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_external_identities_provider_subject_tenant
    ON external_identities (provider_key, subject, tenant_discriminator);

CREATE INDEX IF NOT EXISTS idx_external_identities_user_id
    ON external_identities (user_id);

CREATE TABLE IF NOT EXISTS user_role_assignments (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    role auth_role NOT NULL,
    granted_by_user_id uuid REFERENCES users(id),
    created_at timestamptz NOT NULL DEFAULT NOW(),
    UNIQUE (user_id, role)
);

CREATE INDEX IF NOT EXISTS idx_user_role_assignments_role
    ON user_role_assignments (role);

CREATE TABLE IF NOT EXISTS user_sessions (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    session_token_hash text NOT NULL UNIQUE,
    issued_at timestamptz NOT NULL DEFAULT NOW(),
    expires_at timestamptz NOT NULL,
    last_seen_at timestamptz NOT NULL DEFAULT NOW(),
    invalidated_at timestamptz,
    user_agent text,
    ip_address varchar(64)
);

CREATE INDEX IF NOT EXISTS idx_user_sessions_user_id
    ON user_sessions (user_id);

CREATE INDEX IF NOT EXISTS idx_user_sessions_expires_at
    ON user_sessions (expires_at);

CREATE TRIGGER update_external_identities_updated_at
    BEFORE UPDATE ON external_identities
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();

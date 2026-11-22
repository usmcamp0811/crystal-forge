-- Owner access tokens (CF-AGENT-ACCESS-001 sections 5 and 6, TASK-534.3).
--
-- Only the SHA-256 of the complete canonical token is stored. The public token
-- id is `id` itself; the token string carries it as 32 lowercase hex digits.
-- Grants (scopes and environments) are immutable after insert: the service
-- never updates or deletes junction rows except through ON DELETE CASCADE.

CREATE TABLE IF NOT EXISTS api_access_tokens (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    name text NOT NULL CHECK (char_length(name) BETWEEN 1 AND 100),
    audience text NOT NULL CHECK (audience = 'mcp'),
    secret_hash bytea NOT NULL UNIQUE CHECK (octet_length(secret_hash) = 32),
    display_hint text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    expires_at timestamptz NOT NULL,
    revoked_at timestamptz,
    -- Deleting the revoking user must not delete or block token history.
    revoked_by_user_id uuid REFERENCES users (id) ON DELETE SET NULL,
    last_used_at timestamptz,
    CHECK (expires_at > created_at)
);

CREATE TABLE IF NOT EXISTS api_access_token_scopes (
    token_id uuid NOT NULL REFERENCES api_access_tokens (id) ON DELETE CASCADE,
    -- The allowlist is repeated at the database boundary. Adding a scope
    -- requires a new migration, never an application-only change.
    scope text NOT NULL CHECK (
        scope IN (
            'cves:read',
            'poams:read',
            'provenance:read',
            'scans:request',
            'poams:write',
            'poams:verify',
            'poams:close'
        )
    ),
    PRIMARY KEY (token_id, scope)
);

CREATE TABLE IF NOT EXISTS api_access_token_environments (
    token_id uuid NOT NULL REFERENCES api_access_tokens (id) ON DELETE CASCADE,
    environment_id uuid NOT NULL REFERENCES environments (id) ON DELETE CASCADE,
    PRIMARY KEY (token_id, environment_id)
);

-- Name uniqueness per owner among non-revoked tokens, case-insensitive.
CREATE UNIQUE INDEX IF NOT EXISTS api_access_tokens_owner_active_name_idx
    ON api_access_tokens (user_id, lower(name))
    WHERE revoked_at IS NULL;

-- Owner list pages and the per-hour create/rotate rate count.
CREATE INDEX IF NOT EXISTS api_access_tokens_user_idx
    ON api_access_tokens (user_id, created_at DESC);

CREATE INDEX IF NOT EXISTS api_access_tokens_expiry_idx
    ON api_access_tokens (expires_at) WHERE revoked_at IS NULL;

CREATE INDEX IF NOT EXISTS api_access_token_env_idx
    ON api_access_token_environments (environment_id, token_id);

-- Recent-event lookup by token public id (`admin_audit_events.target`).
CREATE INDEX IF NOT EXISTS idx_admin_audit_events_target_created_at
    ON admin_audit_events (target, created_at DESC);

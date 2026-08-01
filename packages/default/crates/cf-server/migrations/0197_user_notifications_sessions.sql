ALTER TABLE user_sessions
    ADD COLUMN IF NOT EXISTS auth_source TEXT NOT NULL DEFAULT 'unknown'
        CHECK (auth_source IN ('unknown', 'dev', 'local', 'oidc'));

CREATE INDEX IF NOT EXISTS idx_user_sessions_active_user
    ON user_sessions (user_id, last_seen_at DESC, issued_at DESC)
    WHERE invalidated_at IS NULL;

CREATE TABLE user_notification_preferences (
    user_id UUID PRIMARY KEY
        REFERENCES users(id)
        ON DELETE CASCADE,
    deploy_failures BOOLEAN NOT NULL DEFAULT TRUE,
    build_failures BOOLEAN NOT NULL DEFAULT TRUE,
    critical_cves BOOLEAN NOT NULL DEFAULT TRUE,
    policy_violations BOOLEAN NOT NULL DEFAULT TRUE,
    heartbeat_lost BOOLEAN NOT NULL DEFAULT FALSE,
    weekly_digest BOOLEAN NOT NULL DEFAULT FALSE,
    delivery_channel TEXT NOT NULL DEFAULT 'in_app'
        CHECK (delivery_channel IN ('in_app', 'email', 'both')),
    deploy_failures_email_enabled_at TIMESTAMPTZ,
    build_failures_email_enabled_at TIMESTAMPTZ,
    critical_cves_email_enabled_at TIMESTAMPTZ,
    policy_violations_email_enabled_at TIMESTAMPTZ,
    heartbeat_lost_email_enabled_at TIMESTAMPTZ,
    weekly_digest_enabled_at TIMESTAMPTZ,
    initialized_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE user_notifications (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL
        REFERENCES users(id)
        ON DELETE CASCADE,
    category TEXT NOT NULL
        CHECK (category IN ('deploy_failures', 'build_failures', 'critical_cves', 'policy_violations', 'heartbeat_lost')),
    source_occurrence_id UUID,
    source_type TEXT NOT NULL,
    source_id TEXT NOT NULL,
    title TEXT NOT NULL,
    summary TEXT NOT NULL,
    route TEXT NOT NULL,
    in_app_visible BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    read_at TIMESTAMPTZ,
    dismissed_at TIMESTAMPTZ,
    UNIQUE (user_id, category, source_occurrence_id)
);

CREATE UNIQUE INDEX idx_user_notifications_unique_deployment_source
    ON user_notifications (user_id, category, source_type, source_id)
    WHERE source_type = 'system_event';

CREATE INDEX idx_user_notifications_unread
    ON user_notifications (user_id, created_at DESC)
    WHERE in_app_visible AND read_at IS NULL AND dismissed_at IS NULL;

CREATE INDEX idx_user_notifications_visible
    ON user_notifications (user_id, created_at DESC)
    WHERE in_app_visible AND dismissed_at IS NULL;

CREATE INDEX idx_user_notifications_source_occurrence
    ON user_notifications (source_occurrence_id)
    WHERE source_occurrence_id IS NOT NULL;

CREATE OR REPLACE FUNCTION notification_visible_to_user(
    p_user_id UUID,
    p_source_type TEXT,
    p_source_id TEXT
) RETURNS BOOLEAN
LANGUAGE SQL
STABLE
AS $$
    SELECT EXISTS (
        SELECT 1
        FROM user_role_assignments ura
        WHERE ura.user_id = p_user_id
          AND ura.role = 'admin'
    )
    OR p_source_type NOT IN ('systems', 'system_event')
    OR (
        p_source_type = 'systems'
        AND EXISTS (
            SELECT 1
            FROM systems s
            JOIN user_environment_memberships uem
              ON uem.environment_id = s.environment_id
             AND uem.user_id = p_user_id
            WHERE s.id::text = p_source_id
        )
    )
    OR (
        p_source_type = 'system_event'
        AND EXISTS (
            SELECT 1
            FROM system_events se
            JOIN systems s ON s.id = se.system_id
            JOIN user_environment_memberships uem
              ON uem.environment_id = s.environment_id
             AND uem.user_id = p_user_id
            WHERE se.id::text = p_source_id
        )
    );
$$;

CREATE TABLE user_notification_email_deliveries (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL
        REFERENCES users(id)
        ON DELETE CASCADE,
    notification_id UUID
        REFERENCES user_notifications(id)
        ON DELETE SET NULL,
    delivery_type TEXT NOT NULL CHECK (delivery_type IN ('immediate', 'weekly_digest')),
    idempotency_key TEXT NOT NULL UNIQUE,
    state TEXT NOT NULL DEFAULT 'pending'
        CHECK (state IN ('pending', 'sending', 'sent', 'failed', 'cancelled')),
    attempt_count INTEGER NOT NULL DEFAULT 0 CHECK (attempt_count >= 0),
    next_attempt_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    claimed_at TIMESTAMPTZ,
    sent_at TIMESTAMPTZ,
    last_error TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_user_notification_email_deliveries_claim
    ON user_notification_email_deliveries (state, next_attempt_at, created_at)
    WHERE state IN ('pending', 'sending');

CREATE TABLE user_notification_weekly_digest_runs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL
        REFERENCES users(id)
        ON DELETE CASCADE,
    period_start TIMESTAMPTZ NOT NULL,
    period_end TIMESTAMPTZ NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending', 'sending', 'sent', 'failed', 'skipped')),
    delivery_id UUID
        REFERENCES user_notification_email_deliveries(id)
        ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    sent_at TIMESTAMPTZ,
    error_details TEXT,
    UNIQUE (user_id, period_start, period_end),
    CHECK (period_end > period_start)
);

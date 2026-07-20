-- Canonical, server-owned attention occurrences and per-user dismissals.
--
-- Replaces the mutable per-category baseline in user_alert_acknowledgments with
-- immutable occurrence rows and per-user dismissal rows. An occurrence is
-- attention-eligible for 24 hours from its opened_at timestamp and stops
-- contributing to badges after a user dismisses it or after the window expires.
-- The 24-hour rule is enforced in queries, not by cleanup timing.

CREATE TABLE IF NOT EXISTS attention_occurrences (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    category TEXT NOT NULL,
    subject_type TEXT NOT NULL,
    subject_id TEXT NOT NULL,
    source_occurrence_key TEXT NOT NULL,
    opened_at TIMESTAMPTZ NOT NULL,
    last_observed_at TIMESTAMPTZ NOT NULL,
    resolved_at TIMESTAMPTZ NULL,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    CONSTRAINT attention_occurrences_category_key UNIQUE (category, source_occurrence_key),
    CONSTRAINT attention_occurrences_category_check CHECK (
        category IN ('builds', 'evals', 'flakes', 'systems', 'environments', 'cves')
    )
);

CREATE TABLE IF NOT EXISTS user_attention_dismissals (
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    occurrence_id UUID NOT NULL REFERENCES attention_occurrences(id) ON DELETE CASCADE,
    dismissed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (user_id, occurrence_id)
);

-- Indexes for badge queries, reconciliation, and cleanup.
CREATE INDEX IF NOT EXISTS idx_attention_occurrences_category_opened
    ON attention_occurrences (category, opened_at)
    WHERE resolved_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_attention_occurrences_recent_eligible
    ON attention_occurrences (category, opened_at)
    WHERE resolved_at IS NULL AND opened_at > NOW() - INTERVAL '24 hours';

CREATE INDEX IF NOT EXISTS idx_attention_occurrences_subject
    ON attention_occurrences (subject_type, subject_id);

CREATE INDEX IF NOT EXISTS idx_attention_occurrences_last_observed
    ON attention_occurrences (last_observed_at);

CREATE INDEX IF NOT EXISTS idx_attention_occurrences_resolved_at
    ON attention_occurrences (resolved_at)
    WHERE resolved_at IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_user_attention_dismissals_occurrence
    ON user_attention_dismissals (occurrence_id);

CREATE INDEX IF NOT EXISTS idx_user_attention_dismissals_dismissed_at
    ON user_attention_dismissals (dismissed_at);

COMMENT ON TABLE attention_occurrences IS
    'Server-owned canonical attention occurrences. One row per uninterrupted incident. opened_at is immutable; resolved_at is set once and never cleared.';

COMMENT ON COLUMN attention_occurrences.source_occurrence_key IS
    'Stable identity for the incident within its category. Must not include mutable counts, timestamps, or error text.';

COMMENT ON COLUMN attention_occurrences.metadata IS
    'Safe routing/presentation metadata only. No logs, secrets, credentials, or unredacted error text.';

COMMENT ON TABLE user_attention_dismissals IS
    'Per-user dismissal of a canonical attention occurrence. Durable across sessions, browsers, and devices.';

-- Bounded retention cleanup for resolved occurrences and stale dismissals.
-- Unresolved occurrences are never deleted merely because they are older than
-- 24 hours; cleanup is for audit/deduplication housekeeping and is not required
-- for badge correctness.
CREATE OR REPLACE FUNCTION cleanup_attention_occurrences(
    resolved_retention INTERVAL DEFAULT INTERVAL '30 days',
    batch_size INT DEFAULT 1000
)
RETURNS TABLE (deleted_occurrences BIGINT, deleted_dismissals BIGINT)
LANGUAGE plpgsql
AS $$
DECLARE
    deleted_occ BIGINT;
    deleted_dis BIGINT;
BEGIN
    -- Delete resolved occurrences older than the retention threshold.
    WITH deleted AS (
        DELETE FROM attention_occurrences
        WHERE resolved_at IS NOT NULL
          AND resolved_at < NOW() - resolved_retention
        LIMIT batch_size
        RETURNING id
    )
    SELECT COUNT(*) INTO deleted_occ FROM deleted;

    -- Delete dismissals for occurrences that no longer exist.
    WITH deleted AS (
        DELETE FROM user_attention_dismissals
        WHERE NOT EXISTS (
            SELECT 1 FROM attention_occurrences ao WHERE ao.id = user_attention_dismissals.occurrence_id
        )
        LIMIT batch_size
        RETURNING occurrence_id
    )
    SELECT COUNT(*) INTO deleted_dis FROM deleted;

    RETURN QUERY SELECT deleted_occ, deleted_dis;
END;
$$;

COMMENT ON FUNCTION cleanup_attention_occurrences IS
    'Remove resolved occurrences and orphaned dismissals in bounded batches. Default retention is 30 days.';

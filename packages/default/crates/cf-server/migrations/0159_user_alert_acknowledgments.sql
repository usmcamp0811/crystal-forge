-- Per-user, per-category alert acknowledgment baseline.
--
-- Sidebar/tab attention badges previously showed a raw total count of
-- currently-failing items, and acknowledgment was tracked only in an
-- in-memory frontend signal that reset on every page refresh. This caused
-- the same stale count to reappear on every reload, training users to
-- ignore the badges.
--
-- This table persists, per user and alert category, when the user last
-- acknowledged that category (last_seen_at) and how many attention-worthy
-- items existed at that time (last_seen_count). The server uses this to
-- compute "new since last visit" counts instead of raw totals.
CREATE TABLE IF NOT EXISTS user_alert_acknowledgments (
    user_id UUID NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    category TEXT NOT NULL,
    last_seen_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_seen_count BIGINT NOT NULL DEFAULT 0,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (user_id, category)
);

COMMENT ON TABLE user_alert_acknowledgments IS
    'Per-user, per-category baseline for "new since last visit" alert badge counts (see queries::navigation).';

COMMENT ON COLUMN user_alert_acknowledgments.category IS
    'One of: systems, flakes, environments, builds, evals, cves';

COMMENT ON COLUMN user_alert_acknowledgments.last_seen_at IS
    'Timestamp used for categories with a discrete per-item event time (flakes last_sync_at, build_jobs.completed_at, commits.evaluation_completed_at, CVE first_seen). Items newer than this are counted as "new".';

COMMENT ON COLUMN user_alert_acknowledgments.last_seen_count IS
    'Raw attention count recorded at acknowledgment time, used for categories without a discrete per-item timestamp (systems, environments) where health status is a continuously-recomputed function of heartbeat staleness rather than a discrete event.';

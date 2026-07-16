-- Extend user_alert_acknowledgments with a content fingerprint for
-- count-diff categories (systems, environments).
--
-- The old count-based approach missed replacement failures: if system A
-- recovers while system B goes critical, the total stays the same and the
-- badge remains hidden even though the user never saw system B's alert.
--
-- Fix: store a hash of the *set of alerting item IDs* at acknowledgment time.
-- If the set changes (different IDs, even with the same count) the badge
-- resurfaces.  The count column is retained for backward compatibility; the
-- fingerprint takes precedence when present.
ALTER TABLE user_alert_acknowledgments
    ADD COLUMN IF NOT EXISTS last_seen_fingerprint TEXT;

COMMENT ON COLUMN user_alert_acknowledgments.last_seen_fingerprint IS
    'MD5 of the sorted, newline-joined alerting item IDs at acknowledgment
     time (systems/environments only). Badge re-surfaces when the current
     fingerprint differs, even when last_seen_count is unchanged.';

-- Store the exact alerting item IDs a user acknowledged for count-derived
-- categories (systems/environments).
--
-- Fingerprints can detect that a set changed, but cannot distinguish additions
-- from removals.  For example, acknowledged set {A, B} and current set {B}
-- means A recovered and nothing new failed; the badge should stay hidden.
-- Store the acknowledged IDs so navigation badges can compute:
--
--   current_alerting_ids - last_seen_alert_ids
--
-- and only surface newly-added alerting objects.
ALTER TABLE user_alert_acknowledgments
    ADD COLUMN IF NOT EXISTS last_seen_alert_ids TEXT[];

COMMENT ON COLUMN user_alert_acknowledgments.last_seen_alert_ids IS
    'Sorted alerting item IDs acknowledged by the user (systems/environments). Badge counts are computed as current_alerting_ids - last_seen_alert_ids so recoveries do not re-surface old alerts.';

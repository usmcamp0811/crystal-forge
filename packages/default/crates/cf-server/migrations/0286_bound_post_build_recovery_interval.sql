-- 0285 is already applied. Restrict future policy writes to intervals that
-- PostgreSQL can safely cast; never coerce an installed policy value.
ALTER TABLE scan_schedule_policy
    ADD CONSTRAINT scan_schedule_policy_post_build_recovery_interval_bound
        CHECK (
            substring(post_build_recovery_window FROM '^[0-9]+')::numeric
                <= CASE right(post_build_recovery_window, 1)
                    WHEN 'h' THEN 876000
                    ELSE 36500
                END
        );

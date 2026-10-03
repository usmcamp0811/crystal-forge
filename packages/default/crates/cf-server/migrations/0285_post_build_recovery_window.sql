-- The recovery window is a positive number of hours or days. Keep the
-- check at the database boundary for writers other than the schedule API.
ALTER TABLE scan_schedule_policy
    ADD COLUMN post_build_recovery_window varchar(16) NOT NULL DEFAULT '168h',
    ADD CONSTRAINT scan_schedule_policy_post_build_recovery_window_check
        CHECK (
            post_build_recovery_window ~ '^[0-9]{1,15}[hd]$'
            AND substring(post_build_recovery_window FROM '^[0-9]+')::numeric > 0
        );

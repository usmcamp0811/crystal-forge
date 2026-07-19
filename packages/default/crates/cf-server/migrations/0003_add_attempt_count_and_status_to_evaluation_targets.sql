-- Add `attempt_count` to track evaluation attempts
ALTER TABLE tbl_evaluation_targets
    ADD COLUMN attempt_count int NOT NULL DEFAULT 0;

-- Drop the old view to avoid the conflict
DROP VIEW IF EXISTS v_evaluation_target_status;

-- Recreate the view with the updated logic for `status`
CREATE VIEW v_evaluation_target_status AS
SELECT
    *,
    CASE WHEN completed_at IS NOT NULL THEN
        'done'
    WHEN attempt_count >= 5 THEN
        'permanent-failure'
    WHEN scheduled_at IS NOT NULL THEN
        'queued'
    ELSE
        'unscheduled'
    END AS status
FROM
    tbl_evaluation_targets;


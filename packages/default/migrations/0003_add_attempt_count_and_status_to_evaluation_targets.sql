-- Add `attempt_count` to track evaluation attempts
ALTER TABLE tbl_evaluation_targets
    ADD COLUMN attempt_count int NOT NULL DEFAULT 0;

CREATE OR REPLACE VIEW v_evaluation_target_status AS
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


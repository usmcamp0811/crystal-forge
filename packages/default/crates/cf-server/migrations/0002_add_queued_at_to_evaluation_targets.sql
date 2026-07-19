ALTER TABLE tbl_evaluation_targets
    DROP COLUMN queued,
    ADD COLUMN scheduled_at timestamptz DEFAULT now();

ALTER TABLE tbl_evaluation_targets
    ADD COLUMN completed_at timestamptz;

CREATE VIEW v_evaluation_target_status AS
SELECT
    *,
    CASE WHEN completed_at IS NOT NULL THEN
        'done'
    WHEN scheduled_at IS NOT NULL THEN
        'queued'
    ELSE
        'unscheduled'
    END AS status
FROM
    tbl_evaluation_targets;


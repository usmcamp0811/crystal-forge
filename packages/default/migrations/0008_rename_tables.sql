-- Rename tables first
ALTER TABLE tbl_commits RENAME TO commits;

ALTER TABLE tbl_evaluation_targets RENAME TO evaluation_targets;

ALTER TABLE tbl_flakes RENAME TO flakes;

ALTER TABLE tbl_system_states RENAME TO system_states;

-- Add status column to evaluation_targets table
ALTER TABLE evaluation_targets
    ADD COLUMN status text NOT NULL DEFAULT 'queued';


CREATE OR REPLACE VIEW evaluation_targets_with_status AS
SELECT
    et.id,
    et.commit_id,
    et.target_type,
    et.target_name,
    et.derivation_path,
    et.build_timestamp,
    et.scheduled_at,
    et.completed_at,
    et.attempt_count,
    et.status
FROM
    evaluation_targets et
ORDER BY
    et.scheduled_at DESC;

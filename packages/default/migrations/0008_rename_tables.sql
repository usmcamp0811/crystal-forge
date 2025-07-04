-- Rename tables first
ALTER TABLE tbl_commits RENAME TO commits;

ALTER TABLE tbl_evaluation_targets RENAME TO evaluation_targets;

ALTER TABLE tbl_flakes RENAME TO flakes;

ALTER TABLE tbl_system_states RENAME TO system_states;

-- Add the new columns
ALTER TABLE evaluation_targets
    ADD COLUMN status text NOT NULL DEFAULT 'pending';

ALTER TABLE evaluation_targets
    ADD COLUMN started_at timestamp with time zone;

ALTER TABLE evaluation_targets
    ADD COLUMN evaluation_duration_ms integer;

-- Add constraint for valid status values
ALTER TABLE evaluation_targets
    ADD CONSTRAINT valid_status CHECK (status IN ('pending', 'queued', 'in-progress', 'complete', 'failed'));

-- Add indexes for performance
CREATE INDEX idx_evaluation_targets_status ON evaluation_targets (status);


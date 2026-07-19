-- Add to your next migration
ALTER TABLE commits
    ADD COLUMN evaluation_status text DEFAULT 'pending' CHECK (evaluation_status IN ('pending', 'in_progress', 'complete', 'failed')),
    ADD COLUMN evaluation_started_at timestamptz,
    ADD COLUMN evaluation_completed_at timestamptz,
    ADD COLUMN evaluation_attempt_count integer DEFAULT 0,
    ADD COLUMN evaluation_error_message text;

-- Index for querying pending commits
CREATE INDEX idx_commits_evaluation_status ON commits (evaluation_status, evaluation_started_at);


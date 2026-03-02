ALTER TABLE commits
    ADD COLUMN IF NOT EXISTS eval_queue_position bigint;

UPDATE commits
SET eval_queue_position = id
WHERE eval_queue_position IS NULL;

CREATE INDEX IF NOT EXISTS idx_commits_eval_queue_order
    ON commits (evaluation_status, eval_queue_position, commit_timestamp DESC);

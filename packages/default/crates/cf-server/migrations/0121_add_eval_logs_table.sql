-- Migration 0121: Add evaluation logs table for persistent log storage
--
-- This table stores stdout/stderr output from nix-eval-jobs during commit evaluation.
-- Logs are captured in real-time (via WebSocket) and persisted for historical access.
--
-- Design mirrors build_job_logs table structure for consistency.

CREATE TABLE eval_logs (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    commit_id integer NOT NULL REFERENCES commits(id) ON DELETE CASCADE,
    log_timestamp timestamptz NOT NULL DEFAULT now(),
    log_sequence integer NOT NULL,
    log_level text,
    log_message text NOT NULL,
    CONSTRAINT eval_logs_commit_sequence_unique UNIQUE(commit_id, log_sequence)
);

-- Index for efficient log retrieval by commit
CREATE INDEX idx_eval_logs_commit_sequence ON eval_logs(commit_id, log_sequence);

-- Index for log cleanup by age
CREATE INDEX idx_eval_logs_timestamp ON eval_logs(log_timestamp);

COMMENT ON TABLE eval_logs IS 'Persistent storage for evaluation worker stdout/stderr logs';
COMMENT ON COLUMN eval_logs.commit_id IS 'Foreign key to commits table';
COMMENT ON COLUMN eval_logs.log_sequence IS 'Sequential line number within evaluation (1-indexed)';
COMMENT ON COLUMN eval_logs.log_level IS 'Optional log level: info, warn, error, debug (parsed from message or null)';
COMMENT ON COLUMN eval_logs.log_message IS 'Raw log line from nix-eval-jobs or eval worker';

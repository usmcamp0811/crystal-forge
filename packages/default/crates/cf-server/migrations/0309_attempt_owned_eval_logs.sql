-- Historical commit-only rows have no provable owner. Retain them without
-- attributing them to any attempt, including the latest attempt.
ALTER TABLE eval_logs
    ADD COLUMN evaluation_attempt_id uuid,
    ADD COLUMN attempt_number integer,
    ADD CONSTRAINT eval_logs_attempt_attribution_complete CHECK (
        (evaluation_attempt_id IS NULL AND attempt_number IS NULL)
        OR (evaluation_attempt_id IS NOT NULL AND attempt_number IS NOT NULL
            AND attempt_number > 0)
    ),
    ADD CONSTRAINT eval_logs_attempt_owner_fk
        FOREIGN KEY (evaluation_attempt_id, commit_id, attempt_number)
        REFERENCES evaluation_attempts(id, commit_id, attempt_number)
        ON DELETE CASCADE;

ALTER TABLE eval_logs DROP CONSTRAINT eval_logs_commit_sequence_unique;
CREATE UNIQUE INDEX eval_logs_owned_sequence_unique
    ON eval_logs(evaluation_attempt_id, log_sequence)
    WHERE evaluation_attempt_id IS NOT NULL;
CREATE UNIQUE INDEX eval_logs_unattributed_sequence_unique
    ON eval_logs(commit_id, log_sequence)
    WHERE evaluation_attempt_id IS NULL;

COMMENT ON COLUMN eval_logs.evaluation_attempt_id IS
    'Exact producer owner. NULL denotes retained unattributed history, never latest-attempt evidence.';
COMMENT ON COLUMN eval_logs.attempt_number IS
    'Composite attempt ownership, not inferred from the current commit counter.';
COMMENT ON COLUMN eval_logs.log_sequence IS
    'Producer sequence within the exact attempt. Legacy unattributed sequences remain commit-scoped.';

ALTER TABLE commits
    ADD COLUMN IF NOT EXISTS eval_queue_position bigint;

WITH ranked_active AS (
    SELECT
        id,
        ROW_NUMBER() OVER (
            ORDER BY
                CASE WHEN COALESCE(evaluation_status, 'pending') = 'in_progress' THEN 0 ELSE 1 END,
                commit_timestamp DESC,
                id DESC
        )::bigint AS position
    FROM commits
    WHERE COALESCE(evaluation_status, 'pending') IN ('pending', 'in_progress')
),
ranked_terminal AS (
    SELECT
        id,
        ROW_NUMBER() OVER (ORDER BY commit_timestamp DESC, id DESC)::bigint + COALESCE((SELECT MAX(position) FROM ranked_active), 0) AS position
    FROM commits
    WHERE COALESCE(evaluation_status, 'pending') NOT IN ('pending', 'in_progress')
)
UPDATE commits c
SET eval_queue_position = ranked.position
FROM (
    SELECT id, position FROM ranked_active
    UNION ALL
    SELECT id, position FROM ranked_terminal
) ranked
WHERE c.id = ranked.id
  AND c.eval_queue_position IS NULL;

CREATE INDEX IF NOT EXISTS idx_commits_eval_queue_order
    ON commits (evaluation_status, eval_queue_position, commit_timestamp DESC);

CREATE INDEX IF NOT EXISTS idx_commits_eval_queue_active_order
    ON commits (eval_queue_position, commit_timestamp DESC, id DESC)
    WHERE COALESCE(evaluation_status, 'pending') IN ('pending', 'in_progress');

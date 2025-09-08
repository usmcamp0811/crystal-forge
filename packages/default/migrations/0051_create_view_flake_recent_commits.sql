CREATE VIEW view_flake_recent_commits AS
-- Last 3 commits per flake; don't show "retries" if commit has any derivations
WITH ranked AS (
    SELECT
        f.name AS flake,
        c.id AS commit_id,
        c.git_commit_hash AS commit_hash,
        c.commit_timestamp,
        c.attempt_count,
        EXISTS (
            SELECT
                1
            FROM
                derivations d
            WHERE
                d.commit_id = c.id) AS has_derivations,
        NOW() - c.commit_timestamp AS age,
        EXTRACT(EPOCH FROM (NOW() - c.commit_timestamp)) / 60.0 AS minutes_ago,
        ROW_NUMBER() OVER (PARTITION BY f.name ORDER BY c.commit_timestamp DESC) AS rn
    FROM
        commits c
        JOIN flakes f ON f.id = c.flake_id
)
SELECT
    flake,
    LEFT (commit_hash,
        12) AS
COMMIT,
commit_timestamp,
attempt_count,
CASE WHEN attempt_count >= 5 THEN
    '⚠︎ failed/stuck threshold'
WHEN attempt_count > 0
    AND NOT has_derivations THEN
    'retries'
ELSE
    'ok'
END AS attempt_status,
ROUND(minutes_ago)::int AS minutes_since_commit,
    age AS age_interval
FROM
    ranked
WHERE
    rn <= 3
ORDER BY
    flake,
    commit_timestamp DESC;


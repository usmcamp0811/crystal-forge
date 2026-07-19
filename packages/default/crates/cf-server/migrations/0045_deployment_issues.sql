CREATE OR REPLACE VIEW view_deployment_issues AS
WITH current_deployments AS (
    -- What each system is currently running
    SELECT DISTINCT ON (ss.hostname)
        ss.hostname,
        d.commit_id AS current_commit_id,
        c.git_commit_hash AS current_commit_hash,
        c.commit_timestamp AS current_commit_time
    FROM
        system_states ss
        JOIN derivations d ON d.derivation_path = ss.derivation_path
            AND d.derivation_type = 'nixos'
        JOIN commits c ON d.commit_id = c.id
    ORDER BY
        ss.hostname,
        ss.timestamp DESC
),
latest_available AS (
    -- Latest commit available for each system's flake
    SELECT DISTINCT ON (s.hostname)
        s.hostname,
        c.id AS latest_commit_id,
        c.git_commit_hash AS latest_commit_hash,
        c.commit_timestamp AS latest_commit_time
    FROM
        systems s
        JOIN commits c ON s.flake_id = c.flake_id
    ORDER BY
        s.hostname,
        c.commit_timestamp DESC
),
commit_counts AS (
    -- Count commits between current and latest
    SELECT
        cd.hostname,
        COUNT(*) AS commits_between
    FROM
        current_deployments cd
        JOIN latest_available la ON cd.hostname = la.hostname
        JOIN systems s ON cd.hostname = s.hostname
        JOIN commits c ON s.flake_id = c.flake_id
    WHERE
        c.commit_timestamp > cd.current_commit_time
        AND c.commit_timestamp <= la.latest_commit_time
    GROUP BY
        cd.hostname
)
SELECT
    COALESCE(cd.hostname, la.hostname, s.hostname) AS hostname,
    CASE WHEN cd.hostname IS NULL THEN
        'Never Deployed'
    WHEN la.latest_commit_id IS NULL THEN
        'No Commits Available'
    WHEN cd.current_commit_id = la.latest_commit_id THEN
        'Up to Date'
    WHEN cd.current_commit_time < la.latest_commit_time THEN
        'Behind'
    ELSE
        'Unknown'
    END AS deployment_status,
    COALESCE(cc.commits_between, 0) AS commits_behind,
    LEFT (cd.current_commit_hash,
        8) AS current_commit,
    LEFT (la.latest_commit_hash,
        8) AS latest_commit
FROM
    systems s
    LEFT JOIN current_deployments cd ON s.hostname = cd.hostname
    LEFT JOIN latest_available la ON s.hostname = la.hostname
    LEFT JOIN commit_counts cc ON s.hostname = cc.hostname
WHERE
    s.is_active = TRUE
ORDER BY
    CASE WHEN cd.hostname IS NULL THEN
        1 -- Never deployed first
    WHEN la.latest_commit_id IS NULL THEN
        2 -- No commits second
    WHEN cd.current_commit_time < la.latest_commit_time THEN
        3 -- Behind third
    WHEN cd.current_commit_id = la.latest_commit_id THEN
        4 -- Up to date last
    ELSE
        5
    END,
    commits_behind DESC;


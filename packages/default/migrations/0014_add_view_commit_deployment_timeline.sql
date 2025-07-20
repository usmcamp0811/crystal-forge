CREATE OR REPLACE VIEW view_commit_deployment_timeline AS
SELECT
    c.id AS commit_id, -- 1
    c.git_commit_hash, -- 2
    LEFT (c.git_commit_hash,
        8) AS short_hash, -- 3
    c.commit_timestamp, -- 4
    f.name AS flake_name, -- 5
    MIN(ss.timestamp) AS first_deployment, -- 6
    MAX(ss.timestamp) AS last_deployment, -- 7
    COUNT(DISTINCT ss.hostname) AS total_systems_deployed, -- 8
    COUNT(DISTINCT current_sys.hostname) AS currently_deployed_systems, -- 9
    STRING_AGG(DISTINCT ss.hostname, ', ') AS deployed_systems, -- 10
    STRING_AGG(DISTINCT CASE WHEN current_sys.hostname IS NOT NULL THEN
            current_sys.hostname
        END, ', ') AS currently_deployed_systems_list -- 11
FROM
    commits c
    JOIN flakes f ON c.flake_id = f.id
    JOIN evaluation_targets et ON c.id = et.commit_id
        AND et.status = 'complete'
    LEFT JOIN system_states ss ON et.derivation_path = ss.derivation_path
    LEFT JOIN (
        -- Current system states subquery
        SELECT DISTINCT ON (hostname)
            hostname,
            derivation_path
        FROM
            system_states
        ORDER BY
            hostname,
            timestamp DESC) current_sys ON et.derivation_path = current_sys.derivation_path
    AND et.target_name = current_sys.hostname
WHERE
    c.commit_timestamp >= NOW() - INTERVAL '30 days'
GROUP BY
    c.id,
    c.git_commit_hash,
    c.commit_timestamp,
    f.name,
    f.repo_url
ORDER BY
    c.commit_timestamp DESC;


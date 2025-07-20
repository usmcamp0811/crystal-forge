CREATE OR REPLACE VIEW view_commit_deployment_timeline AS
SELECT
    c.flake_id,
    f.name AS flake_name,
    c.id AS commit_id,
    c.git_commit_hash,
    LEFT (c.git_commit_hash,
        8) AS short_hash,
    c.commit_timestamp,
    MIN(ss.timestamp) AS first_deployment,
    MAX(ss.timestamp) AS last_deployment,
    COUNT(DISTINCT ss.hostname) AS total_systems_deployed,
    COUNT(DISTINCT current_sys.hostname) AS currently_deployed_systems,
    STRING_AGG(DISTINCT ss.hostname, ', ') AS deployed_systems,
    STRING_AGG(DISTINCT CASE WHEN current_sys.hostname IS NOT NULL THEN
            current_sys.hostname
        END, ', ') AS currently_deployed_systems_list
FROM
    commits c
    JOIN flakes f ON c.flake_id = f.id
    JOIN evaluation_targets et ON c.id = et.commit_id
        AND et.status = 'complete'
    LEFT JOIN system_states ss ON et.derivation_path = ss.derivation_path
    LEFT JOIN ( SELECT DISTINCT ON (hostname)
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
    c.flake_id,
    c.id,
    c.git_commit_hash,
    c.commit_timestamp,
    f.name
ORDER BY
    c.commit_timestamp DESC;


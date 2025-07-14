INSERT INTO daily_security_posture (snapshot_date, total_systems, systems_with_tpm, systems_secure_boot, systems_fips_mode, systems_selinux_enforcing, systems_agent_compatible, unique_agent_versions, outdated_agent_count)
WITH latest_system_states AS (
    SELECT DISTINCT ON (hostname)
        hostname,
        tpm_present,
        secure_boot_enabled,
        fips_mode,
        selinux_status,
        agent_compatible,
        agent_version
    FROM
        system_states
    ORDER BY
        hostname,
        timestamp DESC),
    agent_version_stats AS (
        SELECT
            agent_version,
            COUNT(*) AS count,
            ROW_NUMBER() OVER (ORDER BY COUNT(*) DESC) AS popularity_rank
        FROM
            latest_system_states
        WHERE
            agent_version IS NOT NULL
        GROUP BY
            agent_version
)
    SELECT
        CURRENT_DATE AS snapshot_date,
        COUNT(*) AS total_systems,
    COUNT(*) FILTER (WHERE tmp_present = TRUE) AS systems_with_tpm,
    COUNT(*) FILTER (WHERE secure_boot_enabled = TRUE) AS systems_secure_boot,
    COUNT(*) FILTER (WHERE fips_mode = TRUE) AS systems_fips_mode,
    COUNT(*) FILTER (WHERE selinux_status = 'Enforcing') AS systems_selinux_enforcing,
    COUNT(*) FILTER (WHERE agent_compatible = TRUE) AS systems_agent_compatible,
    (
        SELECT
            COUNT(DISTINCT agent_version)
        FROM
            latest_system_states
        WHERE
            agent_version IS NOT NULL) AS unique_agent_versions,
    COUNT(*) FILTER (WHERE lss.agent_version NOT IN (
    SELECT
        agent_version FROM agent_version_stats WHERE popularity_rank = 1)) AS outdated_agent_count
FROM
    latest_system_states lss
ON CONFLICT (snapshot_date)
    DO UPDATE SET
        total_systems = EXCLUDED.total_systems,
        systems_with_tpm = EXCLUDED.systems_with_tpm,
        systems_secure_boot = EXCLUDED.systems_secure_boot,
        systems_fips_mode = EXCLUDED.systems_fips_mode,
        systems_selinux_enforcing = EXCLUDED.systems_selinux_enforcing,
        systems_agent_compatible = EXCLUDED.systems_agent_compatible,
        unique_agent_versions = EXCLUDED.unique_agent_versions,
        outdated_agent_count = EXCLUDED.outdated_agent_count,
        created_at = NOW();


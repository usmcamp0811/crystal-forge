-- TASK-225: Track expected Nix store paths from eval for pre-build deployment matching.
-- Also rebuilds view_system_detail to expose the expected_store_path for the
-- latest evaluated derivation for each system.
--
-- After nix-eval-jobs evaluation we know the .drv path. Running
--   nix-store --query --outputs <drv>
-- yields the expected output store path without building. We persist this so that
-- the deployment-status view can match a system's /run/current-system against the
-- expected path even before the builder has completed the real build.
--
-- Semantics:
--   expected_store_path  — set at eval-complete time; the output path Nix would produce.
--   store_path           — set at build-complete time; the path that was actually built.
--
-- The deployment matching view uses COALESCE(store_path, expected_store_path) so
-- an agent-reported /run/current-system path can be correlated as soon as eval
-- finishes, not only after a full build.

ALTER TABLE derivations
    ADD COLUMN IF NOT EXISTS expected_store_path TEXT;

-- Index: used in view_system_deployment_status join on the coalesced path value.
CREATE INDEX IF NOT EXISTS idx_derivations_expected_store_path
    ON derivations (expected_store_path)
    WHERE expected_store_path IS NOT NULL;

-- Rebuild view_system_deployment_status to match on
-- COALESCE(d.store_path, d.expected_store_path).
-- Use CREATE OR REPLACE (no CASCADE) so dependent views are preserved.
CREATE OR REPLACE VIEW view_system_deployment_status AS
WITH latest_system_states AS (
    SELECT DISTINCT ON (hostname)
        hostname,
        store_path,
        timestamp AS deployment_time
    FROM system_states
    ORDER BY hostname, timestamp DESC
),
system_current_derivations AS (
    SELECT
        lss.hostname,
        lss.store_path,
        lss.deployment_time,
        d.id AS derivation_id,
        d.commit_id AS current_commit_id,
        c.git_commit_hash AS current_commit_hash,
        c.commit_timestamp AS current_commit_timestamp,
        c.flake_id,
        f.name AS flake_name
    FROM latest_system_states lss
    -- Match agent-reported store path against either the built path or the expected
    -- path so we can track alignment before the builder has completed.
    LEFT JOIN derivations d
        ON lss.store_path = COALESCE(d.store_path, d.expected_store_path)
    LEFT JOIN commits c ON d.commit_id = c.id
    LEFT JOIN flakes f ON c.flake_id = f.id
),
latest_flake_commits AS (
    SELECT DISTINCT ON (s.hostname)
        s.hostname,
        c.id AS latest_commit_id,
        c.git_commit_hash AS latest_commit_hash,
        c.commit_timestamp AS latest_commit_timestamp,
        c.flake_id
    FROM systems s
    JOIN flakes f ON s.flake_id = f.id
    JOIN commits c ON f.id = c.flake_id
    ORDER BY s.hostname, c.commit_timestamp DESC
),
commit_counts AS (
    SELECT
        scd.hostname,
        COUNT(newer_commits.id) AS commits_behind
    FROM system_current_derivations scd
    JOIN latest_flake_commits lfc ON scd.hostname = lfc.hostname
    LEFT JOIN commits newer_commits
        ON  newer_commits.flake_id = lfc.flake_id
        AND newer_commits.commit_timestamp > scd.current_commit_timestamp
        AND newer_commits.commit_timestamp <= lfc.latest_commit_timestamp
    WHERE scd.current_commit_id IS NOT NULL
      AND lfc.latest_commit_id  IS NOT NULL
    GROUP BY scd.hostname
)
SELECT
    COALESCE(s.hostname, scd.hostname, lfc.hostname) AS hostname,
    scd.store_path AS current_store_path,
    scd.deployment_time,
    scd.current_commit_hash,
    scd.current_commit_timestamp,
    lfc.latest_commit_hash,
    lfc.latest_commit_timestamp,
    COALESCE(cc.commits_behind, 0) AS commits_behind,
    scd.flake_name,
    CASE
        WHEN scd.hostname IS NULL                                            THEN 'no_deployment'
        WHEN scd.flake_id IS NULL                                            THEN 'unknown'
        WHEN scd.current_commit_id = lfc.latest_commit_id                   THEN 'up_to_date'
        WHEN scd.current_commit_id != lfc.latest_commit_id
         AND scd.current_commit_timestamp < lfc.latest_commit_timestamp      THEN 'behind'
        WHEN scd.current_commit_timestamp > lfc.latest_commit_timestamp      THEN 'ahead'
        ELSE 'unknown'
    END AS deployment_status,
    CASE
        WHEN scd.hostname IS NULL                                            THEN 'System registered but never deployed'
        WHEN scd.flake_id IS NULL                                            THEN 'Cannot determine flake relationship'
        WHEN scd.current_commit_id = lfc.latest_commit_id                   THEN 'Running latest commit'
        WHEN scd.current_commit_id != lfc.latest_commit_id
         AND scd.current_commit_timestamp < lfc.latest_commit_timestamp      THEN CONCAT('Behind by ', COALESCE(cc.commits_behind, 0), ' commit(s)')
        WHEN scd.current_commit_timestamp > lfc.latest_commit_timestamp      THEN 'Running newer commit than expected'
        ELSE 'Deployment status unclear'
    END AS status_description
FROM systems s
FULL OUTER JOIN system_current_derivations scd ON s.hostname = scd.hostname
LEFT JOIN latest_flake_commits lfc ON COALESCE(s.hostname, scd.hostname) = lfc.hostname
LEFT JOIN commit_counts cc          ON COALESCE(s.hostname, scd.hostname) = cc.hostname
WHERE s.is_active = TRUE OR s.is_active IS NULL
ORDER BY
    CASE
        WHEN scd.hostname IS NULL                                            THEN 1
        WHEN scd.flake_id IS NULL                                            THEN 2
        WHEN scd.current_commit_id != lfc.latest_commit_id
         AND scd.current_commit_timestamp < lfc.latest_commit_timestamp      THEN 3
        WHEN scd.current_commit_id = lfc.latest_commit_id                   THEN 4
        WHEN scd.current_commit_timestamp > lfc.latest_commit_timestamp      THEN 5
        ELSE 6
    END,
    commits_behind DESC NULLS LAST,
    hostname;

-- Rebuild view_system_detail to include expected_store_path.
-- We add a CTE that finds the latest derivation for each system (by matching
-- the system hostname to derivation_target) and exposes its expected_store_path.
-- This depends on view_system_deployment_status which was already recreated above.
-- Use CREATE OR REPLACE (no CASCADE) so dependent views are preserved.
CREATE OR REPLACE VIEW public.view_system_detail AS
WITH latest_system_state AS (
    SELECT DISTINCT ON (s.id)
        s.id AS system_id,
        ss.id AS system_state_id,
        ss.store_path,
        ss.os,
        ss.kernel,
        ss.nixos_version,
        ss.cpu_brand,
        ss.cpu_cores,
        ss.memory_gb,
        ss.uptime_secs,
        ss.board_serial,
        ss.chassis_serial,
        ss.bios_version,
        ss.cpu_microcode,
        ss.primary_ip_address,
        ss.primary_mac_address,
        ss.gateway_ip,
        ss.network_interfaces,
        ss.tpm_present,
        ss.secure_boot_enabled,
        ss.fips_mode,
        ss.selinux_status,
        ss.agent_version,
        ss.agent_build_hash,
        ss.timestamp AS last_seen
    FROM systems s
    LEFT JOIN system_states ss ON ss.hostname = s.hostname
    ORDER BY s.id, ss.timestamp DESC
),
latest_heartbeat AS (
    SELECT DISTINCT ON (s.id)
        s.id AS system_id,
        ah.timestamp AS heartbeat_timestamp,
        ah.agent_version AS heartbeat_agent_version
    FROM systems s
    LEFT JOIN system_states ss ON ss.hostname = s.hostname
    LEFT JOIN agent_heartbeats ah ON ah.system_state_id = ss.id
    ORDER BY s.id, ah.timestamp DESC NULLS LAST
),
hardware_change_detection AS (
    SELECT
        s.id AS system_id,
        EXISTS (
            SELECT 1
            FROM system_states ss_recent
            WHERE ss_recent.hostname = s.hostname
              AND ss_recent.timestamp >= NOW() - INTERVAL '24 hours'
              AND (
                  ss_recent.cpu_brand IS DISTINCT FROM lss.cpu_brand
                  OR ss_recent.cpu_cores IS DISTINCT FROM lss.cpu_cores
                  OR ss_recent.memory_gb IS DISTINCT FROM lss.memory_gb
                  OR ss_recent.board_serial IS DISTINCT FROM lss.board_serial
                  OR ss_recent.chassis_serial IS DISTINCT FROM lss.chassis_serial
                  OR ss_recent.bios_version IS DISTINCT FROM lss.bios_version
              )
        ) AS hardware_changed_24h,
        EXISTS (
            SELECT 1
            FROM (
                SELECT DISTINCT ON (ss_history.hostname)
                    ss_history.cpu_brand,
                    ss_history.cpu_cores,
                    ss_history.memory_gb,
                    ss_history.board_serial,
                    ss_history.chassis_serial,
                    ss_history.bios_version
                FROM system_states ss_history
                WHERE ss_history.hostname = s.hostname
                ORDER BY ss_history.hostname, ss_history.timestamp ASC
            ) first_state
            WHERE (
                first_state.cpu_brand IS DISTINCT FROM lss.cpu_brand
                OR first_state.cpu_cores IS DISTINCT FROM lss.cpu_cores
                OR first_state.memory_gb IS DISTINCT FROM lss.memory_gb
                OR first_state.board_serial IS DISTINCT FROM lss.board_serial
                OR first_state.chassis_serial IS DISTINCT FROM lss.chassis_serial
                OR first_state.bios_version IS DISTINCT FROM lss.bios_version
            )
        ) AS hardware_ever_changed
    FROM systems s
    LEFT JOIN latest_system_state lss ON lss.system_id = s.id
),
deployment_info AS (
    SELECT
        hostname,
        deployment_status,
        current_store_path,
        status_description
    FROM view_system_deployment_status
),
flake_info AS (
    SELECT
        s.id AS system_id,
        f.id AS flake_id,
        f.name AS flake_name,
        f.repo_url AS flake_repo_url,
        (
            SELECT c.git_commit_hash
            FROM commits c
            WHERE c.flake_id = f.id
            ORDER BY c.commit_timestamp DESC
            LIMIT 1
        ) AS latest_commit
    FROM systems s
    LEFT JOIN flakes f ON f.id = s.flake_id
),
-- Latest evaluated derivation for each system.
-- derivation_name holds the plain NixOS configuration name (e.g. "reckless"),
-- which matches systems.hostname directly.
-- derivation_target holds the full flake ref string and must NOT be used here.
latest_derivation AS (
    SELECT DISTINCT ON (d.derivation_name)
        d.derivation_name AS hostname,
        d.expected_store_path
    FROM derivations d
    WHERE d.derivation_type = 'nixos'
      AND d.expected_store_path IS NOT NULL
    ORDER BY d.derivation_name, d.completed_at DESC NULLS LAST, d.id DESC
)
SELECT
    s.id,
    s.hostname,
    e.name AS environment,
    s.is_active,
    s.deployment_policy,
    CASE
        WHEN lh.heartbeat_timestamp IS NULL THEN 'offline'
        WHEN lh.heartbeat_timestamp < NOW() - INTERVAL '4 hours' THEN 'offline'
        WHEN lh.heartbeat_timestamp < NOW() - INTERVAL '1 hour' THEN 'critical'
        WHEN lh.heartbeat_timestamp < NOW() - INTERVAL '15 minutes' THEN 'warning'
        ELSE 'healthy'
    END AS health_status,
    COALESCE(di.deployment_status, 'unknown') AS deployment_status,
    CASE
        WHEN s.flake_id IS NULL THEN 'unknown'
        WHEN lss.store_path IS NULL THEN 'ready_for_build'
        ELSE 'build_complete'
    END AS pipeline_stage,
    lss.nixos_version,
    lss.kernel,
    lss.agent_version,
    lss.store_path AS current_store_path,
    lss.cpu_brand,
    lss.cpu_cores,
    lss.memory_gb,
    lss.uptime_secs,
    lss.board_serial,
    lss.bios_version,
    lss.primary_ip_address,
    lss.primary_mac_address,
    lss.gateway_ip,
    lss.tpm_present,
    lss.secure_boot_enabled,
    lss.fips_mode,
    lss.selinux_status,
    hcd.hardware_changed_24h,
    hcd.hardware_ever_changed,
    0::integer AS critical_cve_count,
    0::integer AS high_cve_count,
    0::integer AS medium_cve_count,
    0::integer AS low_cve_count,
    fi.flake_id,
    fi.flake_name,
    fi.flake_repo_url,
    fi.latest_commit AS flake_latest_commit,
    GREATEST(
        COALESCE(lh.heartbeat_timestamp, '1970-01-01'::timestamptz),
        COALESCE(lss.last_seen, '1970-01-01'::timestamptz)
    ) AS last_seen,
    s.created_at,
    s.updated_at,
    ld.expected_store_path
FROM systems s
LEFT JOIN environments e ON e.id = s.environment_id
LEFT JOIN latest_system_state lss ON lss.system_id = s.id
LEFT JOIN latest_heartbeat lh ON lh.system_id = s.id
LEFT JOIN hardware_change_detection hcd ON hcd.system_id = s.id
LEFT JOIN deployment_info di ON di.hostname = s.hostname
LEFT JOIN flake_info fi ON fi.system_id = s.id
LEFT JOIN latest_derivation ld ON ld.hostname = s.hostname;

-- For each NixOS system (derivation_name), show the latest-commit pipeline state
-- (dry-run → build → cache-push) plus deployment hints.
CREATE OR REPLACE VIEW view_nixos_pipeline_latest_with_deploy AS
WITH nixos_latest AS (
    -- latest commit per NixOS system (by derivation_name; newest commit_timestamp wins)
    SELECT DISTINCT ON (d.derivation_name)
        d.id AS nixos_id,
        d.derivation_name AS system_name,
        d.derivation_path AS nixos_drv_path,
        d.store_path AS nixos_store_path,
        d.status_id AS nixos_status_id,
        ds.name AS nixos_status,
        ds.is_terminal AS nixos_is_terminal,
        ds.is_success AS nixos_is_success,
        c.id AS commit_id,
        c.git_commit_hash,
        c.commit_timestamp
    FROM
        public.derivations d
        LEFT JOIN public.commits c ON c.id = d.commit_id
        JOIN public.derivation_statuses ds ON ds.id = d.status_id
    WHERE
        d.derivation_type = 'nixos'
    ORDER BY
        d.derivation_name,
        c.commit_timestamp DESC NULLS LAST,
        d.id DESC
),
pkg_deps AS (
    -- package dependencies of the latest nixos derivation
    SELECT
        nl.nixos_id,
        p.id AS package_id,
        p.status_id AS package_status_id,
        p.completed_at AS package_completed_at,
        p.started_at AS package_started_at,
        p.attempt_count AS package_attempts,
        p.derivation_name AS package_name,
        p.derivation_path AS package_drv_path,
        p.store_path AS package_store_path,
        pds.name AS package_status,
        pds.is_success AS package_is_success
    FROM
        nixos_latest nl
        JOIN public.derivation_dependencies dd ON dd.derivation_id = nl.nixos_id
        JOIN public.derivations p ON p.id = dd.depends_on_id
            AND p.derivation_type = 'package'
        LEFT JOIN public.derivation_statuses pds ON pds.id = p.status_id
),
pkg_rollup AS (
    SELECT
        nixos_id,
        COUNT(*) AS total_packages,
        COUNT(*) FILTER (WHERE package_is_success = TRUE) AS completed_packages,
        COUNT(*) FILTER (WHERE package_status IN ('build-in-progress', 'in-progress')) AS building_packages,
        COUNT(*) FILTER (WHERE package_status LIKE '%-failed') AS failed_packages
    FROM
        pkg_deps
    GROUP BY
        nixos_id
),
cache_push AS (
    -- cache push jobs for the package deps
    SELECT
        nl.nixos_id,
        COUNT(*) FILTER (WHERE cpj.status = 'pending') AS cache_pending,
        COUNT(*) FILTER (WHERE cpj.status = 'in_progress') AS cache_in_progress,
        COUNT(*) FILTER (WHERE cpj.status = 'completed') AS cache_completed,
        COUNT(*) FILTER (WHERE cpj.status = 'failed') AS cache_failed,
        MAX(cpj.completed_at) AS last_cache_completed_at
    FROM
        nixos_latest nl
        JOIN public.derivation_dependencies dd ON dd.derivation_id = nl.nixos_id
        JOIN public.cache_push_jobs cpj ON cpj.derivation_id = dd.depends_on_id
    GROUP BY
        nl.nixos_id
),
systems_join AS (
    -- active systems record (by hostname == system_name)
    SELECT
        s.hostname,
        s.id AS systems_id,
        s.deployment_policy,
        s.desired_target,
        s.flake_id,
        s.derivation AS systems_derivation_ref
    FROM
        public.systems s
    WHERE
        s.is_active = TRUE
),
heartbeat_latest AS (
    -- last agent heartbeat per hostname (via system_states)
    SELECT
        st.hostname,
        MAX(ah."timestamp") AS last_agent_heartbeat_at
    FROM
        public.system_states st
        JOIN public.agent_heartbeats ah ON ah.system_state_id = st.id
    GROUP BY
        st.hostname
),
deploy_events AS (
    -- last time we saw a state with change_reason = 'cf_deployment'
    SELECT
        st.hostname,
        MAX(st."timestamp") AS last_cf_deployment_at
    FROM
        public.system_states st
    WHERE
        st.change_reason = 'cf_deployment'
    GROUP BY
        st.hostname
),
final AS (
    SELECT
        nl.nixos_id,
        nl.system_name,
        nl.commit_id,
        nl.git_commit_hash,
        nl.commit_timestamp,
        nl.nixos_status_id,
        nl.nixos_status,
        nl.nixos_drv_path,
        nl.nixos_store_path,
        COALESCE(pr.total_packages, 0) AS total_packages,
        COALESCE(pr.completed_packages, 0) AS completed_packages,
        COALESCE(pr.building_packages, 0) AS building_packages,
        COALESCE(pr.failed_packages, 0) AS failed_packages,
        GREATEST (COALESCE(pr.total_packages, 0) - COALESCE(pr.completed_packages, 0) - COALESCE(pr.building_packages, 0) - COALESCE(pr.failed_packages, 0), 0) AS pending_packages,
        COALESCE(cp.cache_pending, 0) AS cache_pending,
        COALESCE(cp.cache_in_progress, 0) AS cache_in_progress,
        COALESCE(cp.cache_completed, 0) AS cache_completed,
        COALESCE(cp.cache_failed, 0) AS cache_failed,
        cp.last_cache_completed_at,
        -- Deployment-ish joins
        sj.systems_id,
        sj.deployment_policy,
        sj.desired_target,
        sj.flake_id,
        sj.systems_derivation_ref,
        hb.last_agent_heartbeat_at,
        de.last_cf_deployment_at,
        -- Stage booleans
        /* Dry-run is "done" if .drv exists or status name says dry-run complete */
        (nl.nixos_drv_path IS NOT NULL
            OR nl.nixos_status = 'dry-run-complete') AS dry_run_done, /* Build considered done if every package dep is success */ (COALESCE(pr.total_packages, 0) > 0
            AND COALESCE(pr.completed_packages, 0) = COALESCE(pr.total_packages, 0)) AS all_packages_built, /* Cache considered "done" if either:
  - every package has a completed cache push job, OR
  - the NixOS derivation status is 'cache-pushed' (system-level push complete) */ ((COALESCE(pr.total_packages, 0) > 0
            AND COALESCE(cp.cache_completed, 0) = COALESCE(pr.total_packages, 0))
        OR nl.nixos_status = 'cache-pushed') AS all_packages_cached_or_system_cached
    FROM
        nixos_latest nl
        LEFT JOIN pkg_rollup pr ON pr.nixos_id = nl.nixos_id
        LEFT JOIN cache_push cp ON cp.nixos_id = nl.nixos_id
        LEFT JOIN systems_join sj ON sj.hostname = nl.system_name
        LEFT JOIN heartbeat_latest hb ON hb.hostname = nl.system_name
        LEFT JOIN deploy_events de ON de.hostname = nl.system_name
)
SELECT
    nixos_id,
    system_name,
    commit_id,
    git_commit_hash,
    commit_timestamp,
    nixos_status,
    nixos_drv_path,
    nixos_store_path,
    total_packages,
    completed_packages,
    building_packages,
    failed_packages,
    pending_packages,
    cache_pending,
    cache_in_progress,
    cache_completed,
    cache_failed,
    last_cache_completed_at,
    deployment_policy,
    desired_target,
    last_agent_heartbeat_at,
    last_cf_deployment_at,
    dry_run_done,
    all_packages_built,
    all_packages_cached_or_system_cached,
    -- Inferred pipeline stage (pre-deploy) plus a simple deploy hint
    CASE WHEN NOT dry_run_done THEN
        'dry_run'
    WHEN NOT all_packages_built
        AND building_packages > 0 THEN
        'building'
    WHEN NOT all_packages_built THEN
        'waiting_to_build'
    WHEN NOT all_packages_cached_or_system_cached
        AND (cache_in_progress > 0
            OR cache_pending > 0) THEN
        'cache_push'
    WHEN NOT all_packages_cached_or_system_cached THEN
        'cache_push_pending'
    ELSE
        'ready_for_deploy'
    END AS inferred_stage,
    CASE WHEN (deployment_policy = 'manual'
        AND desired_target IS NULL) THEN
        'awaiting_manual_release'
    WHEN (deployment_policy = 'auto_latest'
        AND desired_target IS NULL) THEN
        'auto_release_expected'
    WHEN desired_target IS NOT NULL
        AND last_cf_deployment_at IS NULL THEN
        'release_signaled_no_apply_seen'
    ELSE
        NULL
    END AS deploy_hint
FROM
    final
ORDER BY
    commit_timestamp DESC,
    system_name;


DROP VIEW IF EXISTS view_nixos_pipeline_latest_with_deploy CASCADE;

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
)
SELECT
    nl.nixos_id,
    nl.system_name,
    nl.commit_id,
    nl.git_commit_hash,
    nl.commit_timestamp,
    nl.nixos_status,
    nl.nixos_drv_path,
    nl.nixos_store_path,
    sj.systems_id,
    sj.deployment_policy,
    sj.desired_target,
    sj.flake_id,
    sj.systems_derivation_ref,
    hb.last_agent_heartbeat_at,
    de.last_cf_deployment_at,
    -- Inferred pipeline stage based on NixOS derivation status
    CASE WHEN nl.nixos_status IN ('dry-run-pending', 'dry-run-in-progress') THEN
        'dry_run'
    WHEN nl.nixos_status = 'dry-run-failed' THEN
        'dry_run_failed'
    WHEN nl.nixos_status IN ('build-pending', 'build-in-progress') THEN
        'building'
    WHEN nl.nixos_status = 'build-failed' THEN
        'build_failed'
    WHEN nl.nixos_status = 'dry-run-complete'
        AND nl.nixos_drv_path IS NOT NULL THEN
        'ready_for_build'
    WHEN nl.nixos_status = 'build-complete' THEN
        'build_complete'
    WHEN nl.nixos_status IN ('cache-pushed', 'complete') THEN
        'ready_for_deploy'
    ELSE
        'unknown'
    END AS inferred_stage,
    CASE WHEN sj.deployment_policy = 'manual'
        AND sj.desired_target IS NULL THEN
        'awaiting_manual_release'
    WHEN sj.deployment_policy = 'auto_latest'
        AND sj.desired_target IS NULL THEN
        'auto_release_expected'
    WHEN sj.desired_target IS NOT NULL
        AND de.last_cf_deployment_at IS NULL THEN
        'release_signaled_no_apply_seen'
    ELSE
        NULL
    END AS deploy_hint
FROM
    nixos_latest nl
    LEFT JOIN systems_join sj ON sj.hostname = nl.system_name
    LEFT JOIN heartbeat_latest hb ON hb.hostname = nl.system_name
    LEFT JOIN deploy_events de ON de.hostname = nl.system_name
ORDER BY
    nl.commit_timestamp DESC,
    nl.system_name;


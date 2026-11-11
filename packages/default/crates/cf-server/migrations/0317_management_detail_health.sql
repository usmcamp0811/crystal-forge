-- Align view_system_detail with view_system_list (0316). Nonmanaged
-- registrations have no agent heartbeat, so runtime health, deployment
-- status and pipeline stage are 'unknown', never 'offline'. A reference
-- configuration is not a machine; an external system's runtime is unknown
-- unless a parent host reports a separate guest observation.
-- Column order and types are unchanged (CREATE OR REPLACE requirement).
CREATE OR REPLACE VIEW public.view_system_detail AS
 WITH latest_system_state AS (
         SELECT DISTINCT ON (s_1.id) s_1.id AS system_id,
            ss.id AS system_state_id,
            ss.store_path,
            ss.generation,
            ss.generation_matches_current_store_path,
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
            ss."timestamp" AS last_seen
           FROM systems s_1
             LEFT JOIN system_states ss ON ss.hostname = s_1.hostname
          ORDER BY s_1.id, ss."timestamp" DESC NULLS LAST, ss.id DESC
        ), latest_heartbeat AS (
         SELECT s_1.id AS system_id,
            GREATEST(max(ah."timestamp"), max(ss."timestamp")) AS heartbeat_timestamp,
            ( SELECT ah2.agent_version
                   FROM system_states ss2
                     JOIN agent_heartbeats ah2 ON ah2.system_state_id = ss2.id
                  WHERE ss2.hostname = s_1.hostname
                  ORDER BY ah2."timestamp" DESC
                 LIMIT 1) AS heartbeat_agent_version
           FROM systems s_1
             LEFT JOIN system_states ss ON ss.hostname = s_1.hostname
             LEFT JOIN agent_heartbeats ah ON ah.system_state_id = ss.id
          GROUP BY s_1.id
        ), hardware_change_detection AS (
         SELECT s_1.id AS system_id,
            (EXISTS ( SELECT 1
                   FROM system_states ss_recent
                  WHERE ss_recent.hostname = s_1.hostname AND ss_recent."timestamp" >= (now() - '24:00:00'::interval) AND (ss_recent.cpu_brand IS DISTINCT FROM lss_1.cpu_brand OR ss_recent.cpu_cores IS DISTINCT FROM lss_1.cpu_cores OR ss_recent.memory_gb IS DISTINCT FROM lss_1.memory_gb OR ss_recent.board_serial IS DISTINCT FROM lss_1.board_serial OR ss_recent.chassis_serial IS DISTINCT FROM lss_1.chassis_serial OR ss_recent.bios_version IS DISTINCT FROM lss_1.bios_version))) AS hardware_changed_24h,
            (EXISTS ( SELECT 1
                   FROM ( SELECT DISTINCT ON (ss_history.hostname) ss_history.cpu_brand,
                            ss_history.cpu_cores,
                            ss_history.memory_gb,
                            ss_history.board_serial,
                            ss_history.chassis_serial,
                            ss_history.bios_version
                           FROM system_states ss_history
                          WHERE ss_history.hostname = s_1.hostname
                          ORDER BY ss_history.hostname, ss_history."timestamp") first_state
                  WHERE first_state.cpu_brand IS DISTINCT FROM lss_1.cpu_brand OR first_state.cpu_cores IS DISTINCT FROM lss_1.cpu_cores OR first_state.memory_gb IS DISTINCT FROM lss_1.memory_gb OR first_state.board_serial IS DISTINCT FROM lss_1.board_serial OR first_state.chassis_serial IS DISTINCT FROM lss_1.chassis_serial OR first_state.bios_version IS DISTINCT FROM lss_1.bios_version)) AS hardware_ever_changed
           FROM systems s_1
             LEFT JOIN latest_system_state lss_1 ON lss_1.system_id = s_1.id
        ), deployment_info AS (
         SELECT view_system_deployment_status.hostname,
            view_system_deployment_status.deployment_status,
            view_system_deployment_status.current_store_path,
            view_system_deployment_status.status_description
           FROM view_system_deployment_status
        ), flake_info AS (
         SELECT s_1.id AS system_id,
            f.id AS flake_id,
            f.name AS flake_name,
            f.repo_url AS flake_repo_url,
            ( SELECT c.git_commit_hash
                   FROM commits c
                  WHERE c.flake_id = f.id
                  ORDER BY c.commit_timestamp DESC
                 LIMIT 1) AS latest_commit
           FROM systems s_1
             LEFT JOIN flakes f ON f.id = s_1.flake_id
        ), latest_derivation AS (
         SELECT DISTINCT ON (d.derivation_name) d.derivation_name AS hostname,
            d.expected_store_path
           FROM derivations d
          WHERE d.derivation_type = 'nixos'::text AND d.expected_store_path IS NOT NULL
          ORDER BY d.derivation_name, d.completed_at DESC NULLS LAST, d.id DESC
        ), cve_counts AS (
         SELECT v.hostname,
            count(DISTINCT v.cve_id) FILTER (WHERE v.severity::text = 'CRITICAL'::text)::integer AS critical_cve_count,
            count(DISTINCT v.cve_id) FILTER (WHERE v.severity::text = 'HIGH'::text)::integer AS high_cve_count,
            count(DISTINCT v.cve_id) FILTER (WHERE v.severity::text = 'MEDIUM'::text)::integer AS medium_cve_count,
            count(DISTINCT v.cve_id) FILTER (WHERE v.severity::text = 'LOW'::text)::integer AS low_cve_count
           FROM view_system_vulnerabilities v
             JOIN systems s_1 ON s_1.hostname = v.hostname
          WHERE s_1.is_active = true
          GROUP BY v.hostname
        )
 SELECT s.id,
    s.hostname,
    e.name AS environment,
    s.is_active,
    s.deployment_policy,
        CASE
            WHEN s.management_mode <> 'managed'::text THEN 'unknown'::text
            WHEN lh.heartbeat_timestamp IS NULL THEN 'offline'::text
            WHEN lh.heartbeat_timestamp < (now() - '04:00:00'::interval) THEN 'offline'::text
            WHEN lh.heartbeat_timestamp < (now() - '01:00:00'::interval) THEN 'critical'::text
            WHEN lh.heartbeat_timestamp < (now() - '00:15:00'::interval) THEN 'warning'::text
            ELSE 'healthy'::text
        END AS health_status,
    CASE WHEN s.management_mode = 'managed'::text THEN COALESCE(di.deployment_status, 'unknown'::text) ELSE 'unknown'::text END AS deployment_status,
        CASE
            WHEN s.management_mode <> 'managed'::text OR s.flake_id IS NULL THEN 'unknown'::text
            WHEN lss.store_path IS NULL THEN 'ready_for_build'::text
            ELSE 'build_complete'::text
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
    COALESCE(cc.critical_cve_count, 0) AS critical_cve_count,
    COALESCE(cc.high_cve_count, 0) AS high_cve_count,
    COALESCE(cc.medium_cve_count, 0) AS medium_cve_count,
    COALESCE(cc.low_cve_count, 0) AS low_cve_count,
    fi.flake_id,
    fi.flake_name,
    fi.flake_repo_url,
    fi.latest_commit AS flake_latest_commit,
    GREATEST(COALESCE(lh.heartbeat_timestamp, '1970-01-01 00:00:00+00'::timestamp with time zone), COALESCE(lss.last_seen, '1970-01-01 00:00:00+00'::timestamp with time zone)) AS last_seen,
    s.created_at,
    s.updated_at,
    ld.expected_store_path,
    lss.generation,
    lss.generation_matches_current_store_path,
    s.reachability,
    s.fqdn,
    s.system_configuration_name,
    s.heartbeat_interval_secs,
    s.boot_id,
    s.last_restart_type,
    s.last_restart_at
   FROM systems s
     LEFT JOIN environments e ON e.id = s.environment_id
     LEFT JOIN latest_system_state lss ON lss.system_id = s.id
     LEFT JOIN latest_heartbeat lh ON lh.system_id = s.id
     LEFT JOIN hardware_change_detection hcd ON hcd.system_id = s.id
     LEFT JOIN deployment_info di ON di.hostname = s.hostname
     LEFT JOIN flake_info fi ON fi.system_id = s.id
     LEFT JOIN latest_derivation ld ON ld.hostname = s.hostname
     LEFT JOIN cve_counts cc ON cc.hostname = s.hostname;

COMMENT ON VIEW public.view_system_detail IS
    'Per-registration detail. Only managed machines have runtime health, deployment status and pipeline stage; nonmanaged values are unknown, never offline. Configuration evidence is served by the configuration-assurance API.';

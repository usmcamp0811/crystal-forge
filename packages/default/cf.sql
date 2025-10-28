--
-- PostgreSQL database dump
--

\restrict XRsveR7SRnWfmc3cEPIyEAAbKpICAn3FszXzBLnig098aZYVKaqtrTapIW7trhr

-- Dumped from database version 16.10
-- Dumped by pg_dump version 17.6

SET statement_timeout = 0;
SET lock_timeout = 0;
SET idle_in_transaction_session_timeout = 0;
SET transaction_timeout = 0;
SET client_encoding = 'UTF8';
SET standard_conforming_strings = on;
SELECT pg_catalog.set_config('search_path', '', false);
SET check_function_bodies = false;
SET xmloption = content;
SET client_min_messages = warning;
SET row_security = off;

--
-- Name: user_type; Type: TYPE; Schema: public; Owner: crystal_forge
--

CREATE TYPE public.user_type AS ENUM (
    'human',
    'service',
    'system'
);


ALTER TYPE public.user_type OWNER TO crystal_forge;

--
-- Name: get_latest_cve_scan(text); Type: FUNCTION; Schema: public; Owner: crystal_forge
--

CREATE FUNCTION public.get_latest_cve_scan(derivation_name text) RETURNS TABLE(scan_id uuid, completed_at timestamp with time zone, total_vulnerabilities integer, critical_count integer, high_count integer)
    LANGUAGE plpgsql
    AS $$
BEGIN
    RETURN QUERY
    SELECT
        cs.id,
        cs.completed_at,
        cs.total_vulnerabilities,
        cs.critical_count,
        cs.high_count
    FROM
        derivations d
        JOIN derivation_statuses ds ON d.status_id = ds.id
        JOIN cve_scans cs ON d.id = cs.derivation_id
    WHERE
        d.derivation_name = derivation_name
        AND d.derivation_type = 'nixos'
        AND ds.name = 'complete'
        AND cs.completed_at IS NOT NULL
    ORDER BY
        cs.completed_at DESC
    LIMIT 1;
END;
$$;


ALTER FUNCTION public.get_latest_cve_scan(derivation_name text) OWNER TO crystal_forge;

--
-- Name: severity_from_cvss(numeric); Type: FUNCTION; Schema: public; Owner: crystal_forge
--

CREATE FUNCTION public.severity_from_cvss(score numeric) RETURNS character varying
    LANGUAGE plpgsql IMMUTABLE
    AS $$
BEGIN
    IF score IS NULL THEN
        RETURN 'UNKNOWN';
    ELSIF score >= 9.0 THEN
        RETURN 'CRITICAL';
    ELSIF score >= 7.0 THEN
        RETURN 'HIGH';
    ELSIF score >= 4.0 THEN
        RETURN 'MEDIUM';
    ELSE
        RETURN 'LOW';
    END IF;
END;
$$;


ALTER FUNCTION public.severity_from_cvss(score numeric) OWNER TO crystal_forge;

--
-- Name: update_updated_at_column(); Type: FUNCTION; Schema: public; Owner: crystal_forge
--

CREATE FUNCTION public.update_updated_at_column() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$;


ALTER FUNCTION public.update_updated_at_column() OWNER TO crystal_forge;

SET default_tablespace = '';

SET default_table_access_method = heap;

--
-- Name: _sqlx_migrations; Type: TABLE; Schema: public; Owner: crystal_forge
--

CREATE TABLE public._sqlx_migrations (
    version bigint NOT NULL,
    description text NOT NULL,
    installed_on timestamp with time zone DEFAULT now() NOT NULL,
    success boolean NOT NULL,
    checksum bytea NOT NULL,
    execution_time bigint NOT NULL
);


ALTER TABLE public._sqlx_migrations OWNER TO crystal_forge;

--
-- Name: agent_heartbeats; Type: TABLE; Schema: public; Owner: crystal_forge
--

CREATE TABLE public.agent_heartbeats (
    id bigint NOT NULL,
    system_state_id integer NOT NULL,
    "timestamp" timestamp with time zone DEFAULT now() NOT NULL,
    agent_version character varying(50),
    agent_build_hash character varying(64)
);


ALTER TABLE public.agent_heartbeats OWNER TO crystal_forge;

--
-- Name: agent_heartbeats_id_seq; Type: SEQUENCE; Schema: public; Owner: crystal_forge
--

CREATE SEQUENCE public.agent_heartbeats_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


ALTER SEQUENCE public.agent_heartbeats_id_seq OWNER TO crystal_forge;

--
-- Name: agent_heartbeats_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: crystal_forge
--

ALTER SEQUENCE public.agent_heartbeats_id_seq OWNED BY public.agent_heartbeats.id;


--
-- Name: build_reservations; Type: TABLE; Schema: public; Owner: crystal_forge
--

CREATE TABLE public.build_reservations (
    id integer NOT NULL,
    worker_id text NOT NULL,
    derivation_id integer NOT NULL,
    nixos_derivation_id integer,
    reserved_at timestamp with time zone DEFAULT now() NOT NULL,
    heartbeat_at timestamp with time zone DEFAULT now() NOT NULL
);


ALTER TABLE public.build_reservations OWNER TO crystal_forge;

--
-- Name: TABLE build_reservations; Type: COMMENT; Schema: public; Owner: crystal_forge
--

COMMENT ON TABLE public.build_reservations IS 'Tracks which worker is building which derivation. Reservations are temporary and deleted when build completes or fails.';


--
-- Name: COLUMN build_reservations.worker_id; Type: COMMENT; Schema: public; Owner: crystal_forge
--

COMMENT ON COLUMN public.build_reservations.worker_id IS 'Unique identifier for the worker task (e.g., "builder-hostname-worker-0")';


--
-- Name: COLUMN build_reservations.derivation_id; Type: COMMENT; Schema: public; Owner: crystal_forge
--

COMMENT ON COLUMN public.build_reservations.derivation_id IS 'The derivation this worker is currently building';


--
-- Name: COLUMN build_reservations.nixos_derivation_id; Type: COMMENT; Schema: public; Owner: crystal_forge
--

COMMENT ON COLUMN public.build_reservations.nixos_derivation_id IS 'The parent NixOS system this package belongs to (NULL for system builds)';


--
-- Name: COLUMN build_reservations.reserved_at; Type: COMMENT; Schema: public; Owner: crystal_forge
--

COMMENT ON COLUMN public.build_reservations.reserved_at IS 'When this work was claimed by the worker';


--
-- Name: COLUMN build_reservations.heartbeat_at; Type: COMMENT; Schema: public; Owner: crystal_forge
--

COMMENT ON COLUMN public.build_reservations.heartbeat_at IS 'Last heartbeat timestamp - used to detect crashed/hung workers';


--
-- Name: build_reservations_id_seq; Type: SEQUENCE; Schema: public; Owner: crystal_forge
--

CREATE SEQUENCE public.build_reservations_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


ALTER SEQUENCE public.build_reservations_id_seq OWNER TO crystal_forge;

--
-- Name: build_reservations_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: crystal_forge
--

ALTER SEQUENCE public.build_reservations_id_seq OWNED BY public.build_reservations.id;


--
-- Name: cache_push_jobs; Type: TABLE; Schema: public; Owner: crystal_forge
--

CREATE TABLE public.cache_push_jobs (
    id integer NOT NULL,
    derivation_id integer NOT NULL,
    status text DEFAULT 'pending'::text NOT NULL,
    store_path text,
    scheduled_at timestamp with time zone DEFAULT now() NOT NULL,
    started_at timestamp with time zone,
    completed_at timestamp with time zone,
    attempts integer DEFAULT 0 NOT NULL,
    error_message text,
    push_size_bytes bigint,
    push_duration_ms integer,
    cache_destination text,
    CONSTRAINT cache_push_jobs_status_check CHECK ((status = ANY (ARRAY['pending'::text, 'in_progress'::text, 'completed'::text, 'failed'::text])))
);


ALTER TABLE public.cache_push_jobs OWNER TO crystal_forge;

--
-- Name: cache_push_jobs_id_seq; Type: SEQUENCE; Schema: public; Owner: crystal_forge
--

CREATE SEQUENCE public.cache_push_jobs_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


ALTER SEQUENCE public.cache_push_jobs_id_seq OWNER TO crystal_forge;

--
-- Name: cache_push_jobs_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: crystal_forge
--

ALTER SEQUENCE public.cache_push_jobs_id_seq OWNED BY public.cache_push_jobs.id;


--
-- Name: commits; Type: TABLE; Schema: public; Owner: crystal_forge
--

CREATE TABLE public.commits (
    id integer NOT NULL,
    flake_id integer NOT NULL,
    git_commit_hash text NOT NULL,
    commit_timestamp timestamp with time zone NOT NULL,
    attempt_count integer DEFAULT 0 NOT NULL
);


ALTER TABLE public.commits OWNER TO crystal_forge;

--
-- Name: compliance_levels; Type: TABLE; Schema: public; Owner: crystal_forge
--

CREATE TABLE public.compliance_levels (
    id integer NOT NULL,
    name character varying(50) NOT NULL,
    description text
);


ALTER TABLE public.compliance_levels OWNER TO crystal_forge;

--
-- Name: compliance_levels_id_seq; Type: SEQUENCE; Schema: public; Owner: crystal_forge
--

CREATE SEQUENCE public.compliance_levels_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


ALTER SEQUENCE public.compliance_levels_id_seq OWNER TO crystal_forge;

--
-- Name: compliance_levels_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: crystal_forge
--

ALTER SEQUENCE public.compliance_levels_id_seq OWNED BY public.compliance_levels.id;


--
-- Name: cve_scans; Type: TABLE; Schema: public; Owner: crystal_forge
--

CREATE TABLE public.cve_scans (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    derivation_id integer NOT NULL,
    scheduled_at timestamp with time zone DEFAULT now(),
    completed_at timestamp with time zone,
    status character varying(20),
    attempts integer DEFAULT 0,
    scanner_name character varying(50) NOT NULL,
    scanner_version character varying(50),
    total_packages integer DEFAULT 0 NOT NULL,
    total_vulnerabilities integer DEFAULT 0 NOT NULL,
    critical_count integer DEFAULT 0 NOT NULL,
    high_count integer DEFAULT 0 NOT NULL,
    medium_count integer DEFAULT 0 NOT NULL,
    low_count integer DEFAULT 0 NOT NULL,
    scan_duration_ms integer,
    scan_metadata jsonb,
    created_at timestamp with time zone DEFAULT now()
);


ALTER TABLE public.cve_scans OWNER TO crystal_forge;

--
-- Name: cves; Type: TABLE; Schema: public; Owner: crystal_forge
--

CREATE TABLE public.cves (
    id character varying(20) NOT NULL,
    cvss_v3_score numeric(3,1),
    cvss_v2_score numeric(3,1),
    description text,
    published_date date,
    modified_date date,
    vector character varying(100),
    cwe_id character varying(20),
    metadata jsonb,
    created_at timestamp with time zone DEFAULT now(),
    updated_at timestamp with time zone DEFAULT now()
);


ALTER TABLE public.cves OWNER TO crystal_forge;

--
-- Name: daily_compliance_snapshots; Type: TABLE; Schema: public; Owner: crystal_forge
--

CREATE TABLE public.daily_compliance_snapshots (
    snapshot_date date NOT NULL,
    total_systems integer NOT NULL,
    systems_up_to_date integer NOT NULL,
    systems_behind integer NOT NULL,
    systems_no_evaluation integer NOT NULL,
    systems_offline integer NOT NULL,
    systems_never_seen integer NOT NULL,
    compliance_percentage numeric(5,2) NOT NULL,
    created_at timestamp with time zone DEFAULT now()
);


ALTER TABLE public.daily_compliance_snapshots OWNER TO crystal_forge;

--
-- Name: daily_deployment_velocity; Type: TABLE; Schema: public; Owner: crystal_forge
--

CREATE TABLE public.daily_deployment_velocity (
    snapshot_date date NOT NULL,
    new_commits_today integer NOT NULL,
    commits_evaluated_today integer NOT NULL,
    commits_deployed_today integer NOT NULL,
    avg_eval_to_deploy_hours numeric(8,2),
    max_eval_to_deploy_hours numeric(8,2),
    systems_updated_today integer NOT NULL,
    fastest_deployment_minutes integer,
    slowest_deployment_hours numeric(8,2),
    created_at timestamp with time zone DEFAULT now()
);


ALTER TABLE public.daily_deployment_velocity OWNER TO crystal_forge;

--
-- Name: daily_drift_snapshots; Type: TABLE; Schema: public; Owner: crystal_forge
--

CREATE TABLE public.daily_drift_snapshots (
    snapshot_date date NOT NULL,
    hostname character varying(255) NOT NULL,
    drift_hours numeric,
    is_behind boolean,
    created_at timestamp with time zone DEFAULT now()
);


ALTER TABLE public.daily_drift_snapshots OWNER TO crystal_forge;

--
-- Name: daily_evaluation_health; Type: TABLE; Schema: public; Owner: crystal_forge
--

CREATE TABLE public.daily_evaluation_health (
    snapshot_date date NOT NULL,
    total_evaluations integer NOT NULL,
    successful_evaluations integer NOT NULL,
    failed_evaluations integer NOT NULL,
    pending_evaluations integer NOT NULL,
    avg_evaluation_duration_ms integer,
    max_evaluation_duration_ms integer,
    success_rate_percentage numeric(5,2) NOT NULL,
    evaluations_with_retries integer NOT NULL,
    created_at timestamp with time zone DEFAULT now()
);


ALTER TABLE public.daily_evaluation_health OWNER TO crystal_forge;

--
-- Name: daily_heartbeat_health; Type: TABLE; Schema: public; Owner: crystal_forge
--

CREATE TABLE public.daily_heartbeat_health (
    snapshot_date date NOT NULL,
    total_systems integer NOT NULL,
    systems_healthy integer NOT NULL,
    systems_warning integer NOT NULL,
    systems_critical integer NOT NULL,
    systems_offline integer NOT NULL,
    systems_no_heartbeats integer NOT NULL,
    avg_heartbeat_interval_minutes numeric(8,2),
    total_heartbeats_24h integer NOT NULL,
    created_at timestamp with time zone DEFAULT now()
);


ALTER TABLE public.daily_heartbeat_health OWNER TO crystal_forge;

--
-- Name: daily_security_posture; Type: TABLE; Schema: public; Owner: crystal_forge
--

CREATE TABLE public.daily_security_posture (
    snapshot_date date NOT NULL,
    total_systems integer NOT NULL,
    systems_with_tpm integer NOT NULL,
    systems_secure_boot integer NOT NULL,
    systems_fips_mode integer NOT NULL,
    systems_selinux_enforcing integer NOT NULL,
    systems_agent_compatible integer NOT NULL,
    unique_agent_versions integer NOT NULL,
    outdated_agent_count integer NOT NULL,
    created_at timestamp with time zone DEFAULT now()
);


ALTER TABLE public.daily_security_posture OWNER TO crystal_forge;

--
-- Name: derivation_dependencies; Type: TABLE; Schema: public; Owner: crystal_forge
--

CREATE TABLE public.derivation_dependencies (
    derivation_id integer NOT NULL,
    depends_on_id integer NOT NULL
);


ALTER TABLE public.derivation_dependencies OWNER TO crystal_forge;

--
-- Name: derivation_statuses; Type: TABLE; Schema: public; Owner: crystal_forge
--

CREATE TABLE public.derivation_statuses (
    id integer NOT NULL,
    name character varying(50) NOT NULL,
    description text,
    is_terminal boolean DEFAULT false NOT NULL,
    is_success boolean DEFAULT false NOT NULL,
    display_order integer NOT NULL
);


ALTER TABLE public.derivation_statuses OWNER TO crystal_forge;

--
-- Name: derivations; Type: TABLE; Schema: public; Owner: crystal_forge
--

CREATE TABLE public.derivations (
    id integer NOT NULL,
    commit_id integer,
    derivation_type text NOT NULL,
    derivation_name text NOT NULL,
    derivation_path text,
    scheduled_at timestamp with time zone DEFAULT now(),
    completed_at timestamp with time zone,
    attempt_count integer DEFAULT 0 NOT NULL,
    started_at timestamp with time zone,
    evaluation_duration_ms integer,
    error_message text,
    pname character varying(255),
    version character varying(100),
    status_id integer NOT NULL,
    derivation_target text,
    build_elapsed_seconds integer,
    build_current_target text,
    build_last_activity_seconds integer,
    build_last_heartbeat timestamp with time zone,
    cf_agent_enabled boolean DEFAULT false,
    store_path text,
    CONSTRAINT valid_derivation_type CHECK ((derivation_type = ANY (ARRAY['nixos'::text, 'package'::text])))
);


ALTER TABLE public.derivations OWNER TO crystal_forge;

--
-- Name: COLUMN derivations.store_path; Type: COMMENT; Schema: public; Owner: crystal_forge
--

COMMENT ON COLUMN public.derivations.store_path IS 'The actual built output path in the Nix store. Used for cache push operations. Persists after GC unlike derivation_path (.drv files).';


--
-- Name: derivations_id_seq; Type: SEQUENCE; Schema: public; Owner: crystal_forge
--

CREATE SEQUENCE public.derivations_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


ALTER SEQUENCE public.derivations_id_seq OWNER TO crystal_forge;

--
-- Name: derivations_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: crystal_forge
--

ALTER SEQUENCE public.derivations_id_seq OWNED BY public.derivations.id;


--
-- Name: derivations_with_status; Type: VIEW; Schema: public; Owner: crystal_forge
--

CREATE VIEW public.derivations_with_status AS
 SELECT d.id,
    d.commit_id,
    d.derivation_type,
    d.derivation_name,
    d.derivation_path,
    d.scheduled_at,
    d.completed_at,
    d.attempt_count,
    d.started_at,
    d.evaluation_duration_ms,
    d.error_message,
    d.pname,
    d.version,
    d.status_id,
    ds.name AS status_name,
    ds.description AS status_description,
    ds.is_terminal,
    ds.is_success,
    ds.display_order
   FROM (public.derivations d
     JOIN public.derivation_statuses ds ON ((d.status_id = ds.id)));


ALTER VIEW public.derivations_with_status OWNER TO crystal_forge;

--
-- Name: environments; Type: TABLE; Schema: public; Owner: crystal_forge
--

CREATE TABLE public.environments (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    name character varying(50) NOT NULL,
    description text,
    is_active boolean DEFAULT true,
    compliance_level_id integer,
    risk_profile_id integer,
    created_by character varying(100),
    updated_by character varying(100),
    created_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP,
    updated_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP
);


ALTER TABLE public.environments OWNER TO crystal_forge;

--
-- Name: flakes; Type: TABLE; Schema: public; Owner: crystal_forge
--

CREATE TABLE public.flakes (
    id integer NOT NULL,
    name text NOT NULL,
    repo_url text NOT NULL
);


ALTER TABLE public.flakes OWNER TO crystal_forge;

--
-- Name: package_vulnerabilities; Type: TABLE; Schema: public; Owner: crystal_forge
--

CREATE TABLE public.package_vulnerabilities (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    cve_id character varying(20),
    is_whitelisted boolean DEFAULT false,
    whitelist_reason text,
    whitelist_expires_at timestamp with time zone,
    fixed_version character varying(100),
    detection_method character varying(50) DEFAULT 'vulnix'::character varying,
    created_at timestamp with time zone DEFAULT now(),
    updated_at timestamp with time zone DEFAULT now(),
    derivation_id integer NOT NULL
);


ALTER TABLE public.package_vulnerabilities OWNER TO crystal_forge;

--
-- Name: risk_profiles; Type: TABLE; Schema: public; Owner: crystal_forge
--

CREATE TABLE public.risk_profiles (
    id integer NOT NULL,
    name character varying(50) NOT NULL,
    description text
);


ALTER TABLE public.risk_profiles OWNER TO crystal_forge;

--
-- Name: risk_profiles_id_seq; Type: SEQUENCE; Schema: public; Owner: crystal_forge
--

CREATE SEQUENCE public.risk_profiles_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


ALTER SEQUENCE public.risk_profiles_id_seq OWNER TO crystal_forge;

--
-- Name: risk_profiles_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: crystal_forge
--

ALTER SEQUENCE public.risk_profiles_id_seq OWNED BY public.risk_profiles.id;


--
-- Name: scan_packages; Type: TABLE; Schema: public; Owner: crystal_forge
--

CREATE TABLE public.scan_packages (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    scan_id uuid,
    is_runtime_dependency boolean DEFAULT true,
    dependency_depth integer DEFAULT 0,
    created_at timestamp with time zone DEFAULT now(),
    derivation_id integer NOT NULL
);


ALTER TABLE public.scan_packages OWNER TO crystal_forge;

--
-- Name: system_states; Type: TABLE; Schema: public; Owner: crystal_forge
--

CREATE TABLE public.system_states (
    id integer NOT NULL,
    hostname text NOT NULL,
    derivation_path text NOT NULL,
    change_reason text NOT NULL,
    os text,
    kernel text,
    memory_gb double precision,
    uptime_secs bigint,
    cpu_brand text,
    cpu_cores integer,
    board_serial text,
    product_uuid text,
    rootfs_uuid text,
    "timestamp" timestamp with time zone DEFAULT now() NOT NULL,
    chassis_serial text,
    bios_version text,
    cpu_microcode text,
    network_interfaces jsonb,
    primary_mac_address text,
    primary_ip_address text,
    gateway_ip text,
    selinux_status text,
    tpm_present boolean,
    secure_boot_enabled boolean,
    fips_mode boolean,
    agent_version text,
    agent_build_hash text,
    nixos_version text,
    agent_compatible boolean DEFAULT true,
    partial_data boolean DEFAULT false,
    CONSTRAINT valid_change_reason CHECK ((change_reason = ANY (ARRAY['startup'::text, 'config_change'::text, 'state_delta'::text, 'cf_deployment'::text])))
);


ALTER TABLE public.system_states OWNER TO crystal_forge;

--
-- Name: systems; Type: TABLE; Schema: public; Owner: crystal_forge
--

CREATE TABLE public.systems (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    hostname text NOT NULL,
    environment_id uuid,
    is_active boolean DEFAULT true,
    public_key text NOT NULL,
    flake_id integer,
    derivation text NOT NULL,
    created_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP,
    updated_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP,
    desired_target text,
    deployment_policy text DEFAULT 'manual'::text,
    CONSTRAINT systems_deployment_policy_check CHECK ((deployment_policy = ANY (ARRAY['manual'::text, 'auto_latest'::text, 'pinned'::text])))
);


ALTER TABLE public.systems OWNER TO crystal_forge;

--
-- Name: tbl_commits_id_seq; Type: SEQUENCE; Schema: public; Owner: crystal_forge
--

CREATE SEQUENCE public.tbl_commits_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


ALTER SEQUENCE public.tbl_commits_id_seq OWNER TO crystal_forge;

--
-- Name: tbl_commits_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: crystal_forge
--

ALTER SEQUENCE public.tbl_commits_id_seq OWNED BY public.commits.id;


--
-- Name: tbl_flakes_id_seq; Type: SEQUENCE; Schema: public; Owner: crystal_forge
--

CREATE SEQUENCE public.tbl_flakes_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


ALTER SEQUENCE public.tbl_flakes_id_seq OWNER TO crystal_forge;

--
-- Name: tbl_flakes_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: crystal_forge
--

ALTER SEQUENCE public.tbl_flakes_id_seq OWNED BY public.flakes.id;


--
-- Name: tbl_system_states_id_seq; Type: SEQUENCE; Schema: public; Owner: crystal_forge
--

CREATE SEQUENCE public.tbl_system_states_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


ALTER SEQUENCE public.tbl_system_states_id_seq OWNER TO crystal_forge;

--
-- Name: tbl_system_states_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: crystal_forge
--

ALTER SEQUENCE public.tbl_system_states_id_seq OWNED BY public.system_states.id;


--
-- Name: users; Type: TABLE; Schema: public; Owner: crystal_forge
--

CREATE TABLE public.users (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    username character varying(50) NOT NULL,
    first_name character varying(100) NOT NULL,
    last_name character varying(100) NOT NULL,
    email character varying(255) NOT NULL,
    user_type public.user_type DEFAULT 'human'::public.user_type NOT NULL,
    is_active boolean DEFAULT true NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL
);


ALTER TABLE public.users OWNER TO crystal_forge;

--
-- Name: view_build_queue_status; Type: VIEW; Schema: public; Owner: crystal_forge
--

CREATE VIEW public.view_build_queue_status AS
 WITH system_progress AS (
         SELECT d.id AS nixos_id,
            d.derivation_name AS system_name,
            c.commit_timestamp,
            c.git_commit_hash,
            count(DISTINCT p.id) AS total_packages,
            count(DISTINCT p.id) FILTER (WHERE (p.status_id = 6)) AS completed_packages,
            count(DISTINCT p.id) FILTER (WHERE (p.status_id = 8)) AS building_packages,
            count(DISTINCT cpj.id) FILTER (WHERE (cpj.status = 'completed'::text)) AS cached_packages,
            count(DISTINCT br.id) AS active_workers,
            array_agg(DISTINCT br.worker_id) FILTER (WHERE (br.worker_id IS NOT NULL)) AS worker_ids,
            min(br.reserved_at) AS earliest_reservation,
            max(br.heartbeat_at) AS latest_heartbeat
           FROM (((((public.derivations d
             JOIN public.commits c ON ((c.id = d.commit_id)))
             LEFT JOIN public.derivation_dependencies dd ON ((dd.derivation_id = d.id)))
             LEFT JOIN public.derivations p ON (((p.id = dd.depends_on_id) AND (p.derivation_type = 'package'::text))))
             LEFT JOIN public.cache_push_jobs cpj ON ((cpj.derivation_id = p.id)))
             LEFT JOIN public.build_reservations br ON ((br.nixos_derivation_id = d.id)))
          WHERE ((d.derivation_type = 'nixos'::text) AND (d.status_id = ANY (ARRAY[5, 12])))
          GROUP BY d.id, d.derivation_name, c.commit_timestamp, c.git_commit_hash
        )
 SELECT nixos_id,
    system_name,
    commit_timestamp,
    git_commit_hash,
    total_packages,
    completed_packages,
    building_packages,
    ((total_packages - completed_packages) - building_packages) AS pending_packages,
    cached_packages,
    active_workers,
    worker_ids,
    earliest_reservation,
    latest_heartbeat,
        CASE
            WHEN (total_packages = completed_packages) THEN 'ready_for_system_build'::text
            WHEN (active_workers > 0) THEN 'building'::text
            ELSE 'pending'::text
        END AS status,
        CASE
            WHEN ((completed_packages = total_packages) AND (cached_packages < total_packages)) THEN 'waiting_for_cache_push'::text
            ELSE NULL::text
        END AS cache_status,
        CASE
            WHEN ((latest_heartbeat IS NOT NULL) AND (latest_heartbeat < (now() - '00:05:00'::interval))) THEN true
            ELSE false
        END AS has_stale_workers
   FROM system_progress
  ORDER BY commit_timestamp DESC, total_packages;


ALTER VIEW public.view_build_queue_status OWNER TO crystal_forge;

--
-- Name: VIEW view_build_queue_status; Type: COMMENT; Schema: public; Owner: crystal_forge
--

COMMENT ON VIEW public.view_build_queue_status IS 'Monitoring view showing build progress for each NixOS system - for Grafana dashboards';


--
-- Name: view_buildable_derivations; Type: VIEW; Schema: public; Owner: crystal_forge
--

CREATE VIEW public.view_buildable_derivations AS
 WITH system_progress AS (
         SELECT r.nixos_id,
            r.nixos_commit_ts,
            count(DISTINCT p.id) AS total_packages,
            count(DISTINCT p.id) FILTER (WHERE (p.status_id = 6)) AS completed_packages,
            count(DISTINCT cpj.id) FILTER (WHERE (cpj.status = 'completed'::text)) AS cached_packages,
            count(DISTINCT br.id) AS active_workers
           FROM ((((( SELECT d.id AS nixos_id,
                    c.commit_timestamp AS nixos_commit_ts
                   FROM (public.derivations d
                     JOIN public.commits c ON ((c.id = d.commit_id)))
                  WHERE ((d.derivation_type = 'nixos'::text) AND (d.status_id = ANY (ARRAY[5, 12])))) r
             LEFT JOIN public.derivation_dependencies dd ON ((dd.derivation_id = r.nixos_id)))
             LEFT JOIN public.derivations p ON (((p.id = dd.depends_on_id) AND (p.derivation_type = 'package'::text))))
             LEFT JOIN public.cache_push_jobs cpj ON (((cpj.derivation_id = p.id) AND (cpj.status = 'completed'::text))))
             LEFT JOIN public.build_reservations br ON ((br.nixos_derivation_id = r.nixos_id)))
          GROUP BY r.nixos_id, r.nixos_commit_ts
        ), package_candidates AS (
         SELECT p.id,
            p.derivation_name,
            p.derivation_type,
            p.derivation_path,
            p.pname,
            p.version,
            p.status_id,
            sp.nixos_id,
            sp.nixos_commit_ts,
            sp.total_packages,
            sp.completed_packages,
            sp.cached_packages,
            sp.active_workers,
            'package'::text AS build_type,
            row_number() OVER (ORDER BY sp.nixos_commit_ts DESC, sp.total_packages, sp.nixos_id, p.pname) AS queue_position
           FROM (((system_progress sp
             JOIN public.derivation_dependencies dd ON ((dd.derivation_id = sp.nixos_id)))
             JOIN public.derivations p ON ((p.id = dd.depends_on_id)))
             LEFT JOIN public.build_reservations br ON ((br.derivation_id = p.id)))
          WHERE ((p.derivation_type = 'package'::text) AND (p.status_id = ANY (ARRAY[5, 12])) AND (p.attempt_count <= 5) AND (br.id IS NULL))
        ), system_candidates AS (
         SELECT d.id,
            d.derivation_name,
            d.derivation_type,
            d.derivation_path,
            NULL::text AS pname,
            NULL::text AS version,
            d.status_id,
            sp.nixos_id,
            sp.nixos_commit_ts,
            sp.total_packages,
            sp.completed_packages,
            sp.cached_packages,
            sp.active_workers,
            'system'::text AS build_type,
            row_number() OVER (ORDER BY sp.nixos_commit_ts DESC, sp.total_packages, sp.nixos_id) AS queue_position
           FROM ((system_progress sp
             JOIN public.derivations d ON ((d.id = sp.nixos_id)))
             LEFT JOIN public.build_reservations br ON ((br.derivation_id = d.id)))
          WHERE ((d.derivation_type = 'nixos'::text) AND (d.status_id = ANY (ARRAY[5, 12])) AND (d.attempt_count <= 5) AND (br.id IS NULL) AND (sp.total_packages = sp.completed_packages))
        )
 SELECT package_candidates.id,
    package_candidates.derivation_name,
    package_candidates.derivation_type,
    package_candidates.derivation_path,
    package_candidates.pname,
    package_candidates.version,
    package_candidates.status_id,
    package_candidates.nixos_id,
    package_candidates.nixos_commit_ts,
    package_candidates.total_packages,
    package_candidates.completed_packages,
    package_candidates.cached_packages,
    package_candidates.active_workers,
    package_candidates.build_type,
    package_candidates.queue_position
   FROM package_candidates
UNION ALL
 SELECT system_candidates.id,
    system_candidates.derivation_name,
    system_candidates.derivation_type,
    system_candidates.derivation_path,
    system_candidates.pname,
    system_candidates.version,
    system_candidates.status_id,
    system_candidates.nixos_id,
    system_candidates.nixos_commit_ts,
    system_candidates.total_packages,
    system_candidates.completed_packages,
    system_candidates.cached_packages,
    system_candidates.active_workers,
    system_candidates.build_type,
    system_candidates.queue_position
   FROM system_candidates
  ORDER BY 15;


ALTER VIEW public.view_buildable_derivations OWNER TO crystal_forge;

--
-- Name: VIEW view_buildable_derivations; Type: COMMENT; Schema: public; Owner: crystal_forge
--

COMMENT ON VIEW public.view_buildable_derivations IS 'Shows all derivations ready to be claimed by workers, sorted by priority (newest commits first, smallest systems first within commits)';


--
-- Name: view_commit_build_status; Type: VIEW; Schema: public; Owner: crystal_forge
--

CREATE VIEW public.view_commit_build_status AS
 WITH commit_derivation_summary AS (
         SELECT c.id AS commit_id,
            c.flake_id,
            c.git_commit_hash,
            c.commit_timestamp,
            c.attempt_count AS commit_attempt_count,
            f.name AS flake_name,
            f.repo_url,
            d.id AS derivation_id,
            d.derivation_type,
            d.derivation_name,
            d.derivation_path,
            d.scheduled_at AS derivation_scheduled_at,
            d.started_at AS derivation_started_at,
            d.completed_at AS derivation_completed_at,
            d.attempt_count AS derivation_attempt_count,
            d.evaluation_duration_ms,
            d.error_message,
            d.pname,
            d.version,
            ds.name AS derivation_status,
            ds.description AS derivation_status_description,
            ds.is_terminal,
            ds.is_success,
            ds.display_order
           FROM (((public.commits c
             JOIN public.flakes f ON ((c.flake_id = f.id)))
             LEFT JOIN public.derivations d ON ((c.id = d.commit_id)))
             LEFT JOIN public.derivation_statuses ds ON ((d.status_id = ds.id)))
        ), derivation_counts AS (
         SELECT commit_derivation_summary.commit_id,
            count(
                CASE
                    WHEN (commit_derivation_summary.derivation_id IS NOT NULL) THEN 1
                    ELSE NULL::integer
                END) AS total_derivations,
            count(
                CASE
                    WHEN (commit_derivation_summary.is_success = true) THEN 1
                    ELSE NULL::integer
                END) AS successful_derivations,
            count(
                CASE
                    WHEN ((commit_derivation_summary.is_success = false) AND (commit_derivation_summary.is_terminal = true)) THEN 1
                    ELSE NULL::integer
                END) AS failed_derivations,
            count(
                CASE
                    WHEN (commit_derivation_summary.is_terminal = false) THEN 1
                    ELSE NULL::integer
                END) AS in_progress_derivations,
            count(
                CASE
                    WHEN (commit_derivation_summary.derivation_type = 'nixos'::text) THEN 1
                    ELSE NULL::integer
                END) AS nixos_derivations,
            count(
                CASE
                    WHEN (commit_derivation_summary.derivation_type = 'package'::text) THEN 1
                    ELSE NULL::integer
                END) AS package_derivations,
            avg(
                CASE
                    WHEN (commit_derivation_summary.evaluation_duration_ms IS NOT NULL) THEN commit_derivation_summary.evaluation_duration_ms
                    ELSE NULL::integer
                END) AS avg_evaluation_duration_ms,
            max(commit_derivation_summary.derivation_attempt_count) AS max_derivation_attempts,
            string_agg(DISTINCT (commit_derivation_summary.derivation_status)::text, ', '::text ORDER BY (commit_derivation_summary.derivation_status)::text) AS all_statuses
           FROM commit_derivation_summary
          GROUP BY commit_derivation_summary.commit_id
        )
 SELECT cds.commit_id,
    cds.flake_name,
    cds.repo_url,
    cds.git_commit_hash,
    "left"(cds.git_commit_hash, 8) AS short_hash,
    cds.commit_timestamp,
    cds.commit_attempt_count,
    cds.derivation_id,
    cds.derivation_type,
    cds.derivation_name,
    cds.derivation_path,
    cds.derivation_scheduled_at,
    cds.derivation_started_at,
    cds.derivation_completed_at,
    cds.derivation_attempt_count,
    cds.evaluation_duration_ms,
    cds.error_message,
    cds.pname,
    cds.version,
    cds.derivation_status,
    cds.derivation_status_description,
    cds.is_terminal,
    cds.is_success,
    COALESCE(dc.total_derivations, (0)::bigint) AS total_derivations,
    COALESCE(dc.successful_derivations, (0)::bigint) AS successful_derivations,
    COALESCE(dc.failed_derivations, (0)::bigint) AS failed_derivations,
    COALESCE(dc.in_progress_derivations, (0)::bigint) AS in_progress_derivations,
    COALESCE(dc.nixos_derivations, (0)::bigint) AS nixos_derivations,
    COALESCE(dc.package_derivations, (0)::bigint) AS package_derivations,
    round((COALESCE(dc.avg_evaluation_duration_ms, (0)::numeric) / 1000.0), 2) AS avg_evaluation_duration_seconds,
    COALESCE(dc.max_derivation_attempts, 0) AS max_derivation_attempts,
    dc.all_statuses,
        CASE
            WHEN (dc.total_derivations = 0) THEN 'no_builds'::text
            WHEN (dc.in_progress_derivations > 0) THEN 'building'::text
            WHEN ((dc.failed_derivations > 0) AND (dc.successful_derivations = 0)) THEN 'failed'::text
            WHEN ((dc.failed_derivations > 0) AND (dc.successful_derivations > 0)) THEN 'partial'::text
            WHEN (dc.successful_derivations = dc.total_derivations) THEN 'complete'::text
            ELSE 'unknown'::text
        END AS commit_build_status,
        CASE
            WHEN (dc.total_derivations = 0) THEN 'No derivations scheduled for this commit'::text
            WHEN (dc.in_progress_derivations > 0) THEN concat(dc.in_progress_derivations, ' of ', dc.total_derivations, ' derivations still building')
            WHEN ((dc.failed_derivations > 0) AND (dc.successful_derivations = 0)) THEN concat('All ', dc.total_derivations, ' derivations failed')
            WHEN ((dc.failed_derivations > 0) AND (dc.successful_derivations > 0)) THEN concat(dc.successful_derivations, ' successful, ', dc.failed_derivations, ' failed of ', dc.total_derivations, ' total')
            WHEN (dc.successful_derivations = dc.total_derivations) THEN concat('All ', dc.total_derivations, ' derivations completed successfully')
            ELSE 'Build status unclear'::text
        END AS commit_build_description
   FROM (commit_derivation_summary cds
     LEFT JOIN derivation_counts dc ON ((cds.commit_id = dc.commit_id)))
  ORDER BY cds.commit_timestamp DESC, cds.flake_name, cds.derivation_type, cds.derivation_name;


ALTER VIEW public.view_commit_build_status OWNER TO crystal_forge;

--
-- Name: view_commit_deployment_timeline; Type: VIEW; Schema: public; Owner: crystal_forge
--

CREATE VIEW public.view_commit_deployment_timeline AS
 WITH commit_evaluations AS (
         SELECT c.flake_id,
            c.id AS commit_id,
            c.git_commit_hash,
            c.commit_timestamp,
            count(DISTINCT d.id) AS total_evaluations,
            count(DISTINCT d.id) FILTER (WHERE (ds.is_success = true)) AS successful_evaluations,
            string_agg(DISTINCT (ds.name)::text, ', '::text) AS evaluation_statuses,
            string_agg(DISTINCT d.derivation_name, ', '::text) AS evaluated_targets
           FROM ((public.commits c
             LEFT JOIN public.derivations d ON (((c.id = d.commit_id) AND (d.derivation_type = 'nixos'::text))))
             LEFT JOIN public.derivation_statuses ds ON ((d.status_id = ds.id)))
          WHERE (c.commit_timestamp >= (now() - '30 days'::interval))
          GROUP BY c.flake_id, c.id, c.git_commit_hash, c.commit_timestamp
        ), latest_successful_by_system AS (
         SELECT DISTINCT ON (d.derivation_name) d.derivation_name,
            d.commit_id,
            c.git_commit_hash,
            c.commit_timestamp AS derivation_commit_time
           FROM ((public.derivations d
             JOIN public.derivation_statuses ds ON ((d.status_id = ds.id)))
             JOIN public.commits c ON ((d.commit_id = c.id)))
          WHERE ((d.derivation_type = 'nixos'::text) AND (ds.is_success = true) AND (c.commit_timestamp >= (now() - '30 days'::interval)))
          ORDER BY d.derivation_name, c.commit_timestamp DESC
        ), system_first_seen_with_commit AS (
         SELECT lsbs.commit_id,
            lsbs.derivation_name,
            min(ss."timestamp") FILTER (WHERE (ss."timestamp" > lsbs.derivation_commit_time)) AS first_seen_after_commit
           FROM (latest_successful_by_system lsbs
             JOIN public.system_states ss ON ((ss.hostname = lsbs.derivation_name)))
          GROUP BY lsbs.commit_id, lsbs.derivation_name
        ), current_deployments AS (
         SELECT DISTINCT ON (system_states.hostname) system_states.hostname,
            system_states."timestamp" AS "current_timestamp"
           FROM public.system_states
          ORDER BY system_states.hostname, system_states."timestamp" DESC
        ), commit_deployment_stats AS (
         SELECT lsbs.commit_id,
            count(DISTINCT lsbs.derivation_name) AS systems_with_successful_evaluation,
            count(DISTINCT sfswc.derivation_name) AS systems_seen_after_commit,
            min(sfswc.first_seen_after_commit) AS first_system_seen,
            max(sfswc.first_seen_after_commit) AS last_system_seen,
            string_agg(DISTINCT sfswc.derivation_name, ', '::text) AS systems_seen_list,
            count(DISTINCT
                CASE
                    WHEN (cd.hostname IS NOT NULL) THEN lsbs.derivation_name
                    ELSE NULL::text
                END) AS currently_active_systems,
            string_agg(DISTINCT
                CASE
                    WHEN (cd.hostname IS NOT NULL) THEN lsbs.derivation_name
                    ELSE NULL::text
                END, ', '::text) AS currently_active_systems_list
           FROM ((latest_successful_by_system lsbs
             LEFT JOIN system_first_seen_with_commit sfswc ON (((lsbs.commit_id = sfswc.commit_id) AND (lsbs.derivation_name = sfswc.derivation_name))))
             LEFT JOIN current_deployments cd ON ((lsbs.derivation_name = cd.hostname)))
          GROUP BY lsbs.commit_id
        )
 SELECT ce.flake_id,
    f.name AS flake_name,
    ce.commit_id,
    ce.git_commit_hash,
    "left"(ce.git_commit_hash, 8) AS short_hash,
    ce.commit_timestamp,
    ce.total_evaluations,
    ce.successful_evaluations,
    ce.evaluation_statuses,
    ce.evaluated_targets,
    cds.first_system_seen AS first_deployment,
    cds.last_system_seen AS last_deployment,
    COALESCE(cds.systems_seen_after_commit, (0)::bigint) AS total_systems_deployed,
    COALESCE(cds.currently_active_systems, (0)::bigint) AS currently_deployed_systems,
    cds.systems_seen_list AS deployed_systems,
    cds.currently_active_systems_list AS currently_deployed_systems_list
   FROM ((commit_evaluations ce
     JOIN public.flakes f ON ((ce.flake_id = f.id)))
     LEFT JOIN commit_deployment_stats cds ON ((ce.commit_id = cds.commit_id)))
  ORDER BY ce.commit_timestamp DESC;


ALTER VIEW public.view_commit_deployment_timeline OWNER TO crystal_forge;

--
-- Name: VIEW view_commit_deployment_timeline; Type: COMMENT; Schema: public; Owner: crystal_forge
--

COMMENT ON VIEW public.view_commit_deployment_timeline IS 'Shows commit evaluation timeline and approximates deployment by tracking when systems were first seen after commit timestamp.';


--
-- Name: view_commit_nixos_table; Type: VIEW; Schema: public; Owner: crystal_forge
--

CREATE VIEW public.view_commit_nixos_table AS
 WITH base AS (
         SELECT c.id AS commit_id,
            c.git_commit_hash,
            "left"(c.git_commit_hash, 8) AS short_hash,
            c.commit_timestamp,
            f.name AS flake_name,
            d.id AS derivation_id,
            d.derivation_name,
            ds.name AS derivation_status,
            ds.display_order AS status_order,
            ds.is_terminal,
            ds.is_success
           FROM (((public.commits c
             JOIN public.flakes f ON ((f.id = c.flake_id)))
             LEFT JOIN public.derivations d ON (((d.commit_id = c.id) AND (d.derivation_type = 'nixos'::text))))
             LEFT JOIN public.derivation_statuses ds ON ((ds.id = d.status_id)))
        ), agg AS (
         SELECT base.commit_id,
            count(*) FILTER (WHERE (base.derivation_id IS NOT NULL)) AS total,
            count(*) FILTER (WHERE base.is_success) AS successful,
            count(*) FILTER (WHERE (base.is_terminal AND (NOT base.is_success))) AS failed,
            count(*) FILTER (WHERE (NOT base.is_terminal)) AS in_progress
           FROM base
          GROUP BY base.commit_id
        )
 SELECT b.commit_id,
    b.git_commit_hash,
    b.short_hash,
    b.commit_timestamp,
    b.flake_name,
    b.derivation_name,
    b.derivation_status,
    b.status_order,
    a.total,
    a.successful,
    a.failed,
    a.in_progress,
        CASE
            WHEN (a.total > 0) THEN round(((100.0 * (a.successful)::numeric) / (a.total)::numeric), 1)
            ELSE (0)::numeric
        END AS progress_pct
   FROM (base b
     LEFT JOIN agg a USING (commit_id))
  ORDER BY b.commit_timestamp DESC, b.status_order, b.derivation_name;


ALTER VIEW public.view_commit_nixos_table OWNER TO crystal_forge;

--
-- Name: view_commits_stuck_in_evaluation; Type: VIEW; Schema: public; Owner: crystal_forge
--

CREATE VIEW public.view_commits_stuck_in_evaluation AS
 SELECT c.id AS commit_id,
    f.name AS flake_name,
    c.git_commit_hash,
    c.commit_timestamp,
    c.attempt_count
   FROM (public.commits c
     JOIN public.flakes f ON ((c.flake_id = f.id)))
  WHERE (c.attempt_count >= 5)
  ORDER BY c.commit_timestamp DESC;


ALTER VIEW public.view_commits_stuck_in_evaluation OWNER TO crystal_forge;

--
-- Name: view_compliance_trend_7d; Type: VIEW; Schema: public; Owner: crystal_forge
--

CREATE VIEW public.view_compliance_trend_7d AS
 SELECT snapshot_date,
    total_systems,
    systems_up_to_date,
    systems_behind,
    compliance_percentage,
    (compliance_percentage - lag(compliance_percentage) OVER (ORDER BY snapshot_date)) AS daily_change
   FROM public.daily_compliance_snapshots
  WHERE (snapshot_date >= (CURRENT_DATE - '7 days'::interval))
  ORDER BY snapshot_date;


ALTER VIEW public.view_compliance_trend_7d OWNER TO crystal_forge;

--
-- Name: view_system_deployment_status; Type: VIEW; Schema: public; Owner: crystal_forge
--

CREATE VIEW public.view_system_deployment_status AS
 WITH latest_system_states AS (
         SELECT DISTINCT ON (system_states.hostname) system_states.hostname,
            system_states.derivation_path,
            system_states."timestamp" AS deployment_time
           FROM public.system_states
          ORDER BY system_states.hostname, system_states."timestamp" DESC
        ), system_current_derivations AS (
         SELECT lss.hostname,
            lss.derivation_path,
            lss.deployment_time,
            d.id AS derivation_id,
            d.commit_id AS current_commit_id,
            c.git_commit_hash AS current_commit_hash,
            c.commit_timestamp AS current_commit_timestamp,
            c.flake_id,
            f.name AS flake_name
           FROM (((latest_system_states lss
             LEFT JOIN public.derivations d ON ((lss.derivation_path = d.derivation_path)))
             LEFT JOIN public.commits c ON ((d.commit_id = c.id)))
             LEFT JOIN public.flakes f ON ((c.flake_id = f.id)))
        ), latest_flake_commits AS (
         SELECT DISTINCT ON (s_1.hostname) s_1.hostname,
            c.id AS latest_commit_id,
            c.git_commit_hash AS latest_commit_hash,
            c.commit_timestamp AS latest_commit_timestamp,
            c.flake_id
           FROM ((public.systems s_1
             JOIN public.flakes f ON ((s_1.flake_id = f.id)))
             JOIN public.commits c ON ((f.id = c.flake_id)))
          ORDER BY s_1.hostname, c.commit_timestamp DESC
        ), commit_counts AS (
         SELECT scd_1.hostname,
            count(newer_commits.id) AS commits_behind
           FROM ((system_current_derivations scd_1
             JOIN latest_flake_commits lfc_1 ON ((scd_1.hostname = lfc_1.hostname)))
             LEFT JOIN public.commits newer_commits ON (((newer_commits.flake_id = lfc_1.flake_id) AND (newer_commits.commit_timestamp > scd_1.current_commit_timestamp) AND (newer_commits.commit_timestamp <= lfc_1.latest_commit_timestamp))))
          WHERE ((scd_1.current_commit_id IS NOT NULL) AND (lfc_1.latest_commit_id IS NOT NULL))
          GROUP BY scd_1.hostname
        )
 SELECT COALESCE(s.hostname, scd.hostname, lfc.hostname) AS hostname,
    scd.derivation_path AS current_derivation_path,
    scd.deployment_time,
    scd.current_commit_hash,
    scd.current_commit_timestamp,
    lfc.latest_commit_hash,
    lfc.latest_commit_timestamp,
    COALESCE(cc.commits_behind, (0)::bigint) AS commits_behind,
    scd.flake_name,
        CASE
            WHEN (scd.hostname IS NULL) THEN 'no_deployment'::text
            WHEN (scd.flake_id IS NULL) THEN 'unknown'::text
            WHEN (scd.current_commit_id = lfc.latest_commit_id) THEN 'up_to_date'::text
            WHEN ((scd.current_commit_id <> lfc.latest_commit_id) AND (scd.current_commit_timestamp < lfc.latest_commit_timestamp)) THEN 'behind'::text
            WHEN (scd.current_commit_timestamp > lfc.latest_commit_timestamp) THEN 'ahead'::text
            ELSE 'unknown'::text
        END AS deployment_status,
        CASE
            WHEN (scd.hostname IS NULL) THEN 'System registered but never deployed'::text
            WHEN (scd.flake_id IS NULL) THEN 'Cannot determine flake relationship'::text
            WHEN (scd.current_commit_id = lfc.latest_commit_id) THEN 'Running latest commit'::text
            WHEN ((scd.current_commit_id <> lfc.latest_commit_id) AND (scd.current_commit_timestamp < lfc.latest_commit_timestamp)) THEN concat('Behind by ', COALESCE(cc.commits_behind, (0)::bigint), ' commit(s)')
            WHEN (scd.current_commit_timestamp > lfc.latest_commit_timestamp) THEN 'Running newer commit than expected'::text
            ELSE 'Deployment status unclear'::text
        END AS status_description
   FROM (((public.systems s
     FULL JOIN system_current_derivations scd ON ((s.hostname = scd.hostname)))
     LEFT JOIN latest_flake_commits lfc ON ((COALESCE(s.hostname, scd.hostname) = lfc.hostname)))
     LEFT JOIN commit_counts cc ON ((COALESCE(s.hostname, scd.hostname) = cc.hostname)))
  WHERE ((s.is_active = true) OR (s.is_active IS NULL))
  ORDER BY
        CASE
            WHEN (scd.hostname IS NULL) THEN 1
            WHEN (scd.flake_id IS NULL) THEN 2
            WHEN ((scd.current_commit_id <> lfc.latest_commit_id) AND (scd.current_commit_timestamp < lfc.latest_commit_timestamp)) THEN 3
            WHEN (scd.current_commit_id = lfc.latest_commit_id) THEN 4
            WHEN (scd.current_commit_timestamp > lfc.latest_commit_timestamp) THEN 5
            ELSE 6
        END, COALESCE(cc.commits_behind, (0)::bigint) DESC NULLS LAST, COALESCE(s.hostname, scd.hostname, lfc.hostname);


ALTER VIEW public.view_system_deployment_status OWNER TO crystal_forge;

--
-- Name: view_config_timeline; Type: VIEW; Schema: public; Owner: crystal_forge
--

CREATE VIEW public.view_config_timeline AS
 WITH current_counts AS (
         SELECT s.current_commit_hash,
            s.flake_name,
            count(*) FILTER (WHERE ((s.current_derivation_path IS NOT NULL) AND (s.deployment_status = ANY (ARRAY['up_to_date'::text, 'behind'::text, 'ahead'::text])))) AS systems_deployed_now
           FROM public.view_system_deployment_status s
          GROUP BY s.current_commit_hash, s.flake_name
        )
 SELECT v.commit_timestamp AS "time",
    concat(COALESCE(cc.systems_deployed_now, (0)::bigint), ' deployed (', v.short_hash, ')§', (row_number() OVER (PARTITION BY v.flake_name ORDER BY v.commit_timestamp DESC) - 1)) AS "Config",
    v.flake_name
   FROM (public.view_commit_deployment_timeline v
     LEFT JOIN current_counts cc ON (((cc.current_commit_hash = v.git_commit_hash) AND (cc.flake_name = v.flake_name))))
  ORDER BY v.commit_timestamp DESC;


ALTER VIEW public.view_config_timeline OWNER TO crystal_forge;

--
-- Name: VIEW view_config_timeline; Type: COMMENT; Schema: public; Owner: crystal_forge
--

COMMENT ON VIEW public.view_config_timeline IS 'Shows commit evaluation timeline in a formate to be easily parsed into the config timeline in Grafana.';


--
-- Name: view_cve_trends_7d; Type: VIEW; Schema: public; Owner: crystal_forge
--

CREATE VIEW public.view_cve_trends_7d AS
 SELECT date(completed_at) AS scan_date,
    count(DISTINCT derivation_id) AS targets_scanned,
    avg(total_vulnerabilities) AS avg_vulnerabilities,
    avg(critical_count) AS avg_critical,
    avg(high_count) AS avg_high,
    sum(total_vulnerabilities) AS total_vulnerabilities,
    sum(critical_count) AS total_critical,
    sum(high_count) AS total_high
   FROM public.cve_scans cs
  WHERE ((completed_at >= (CURRENT_DATE - '7 days'::interval)) AND (completed_at IS NOT NULL))
  GROUP BY (date(completed_at))
  ORDER BY (date(completed_at));


ALTER VIEW public.view_cve_trends_7d OWNER TO crystal_forge;

--
-- Name: view_deployment_issues; Type: VIEW; Schema: public; Owner: crystal_forge
--

CREATE VIEW public.view_deployment_issues AS
 WITH current_deployments AS (
         SELECT DISTINCT ON (ss.hostname) ss.hostname,
            d.commit_id AS current_commit_id,
            c.git_commit_hash AS current_commit_hash,
            c.commit_timestamp AS current_commit_time
           FROM ((public.system_states ss
             JOIN public.derivations d ON (((d.derivation_path = ss.derivation_path) AND (d.derivation_type = 'nixos'::text))))
             JOIN public.commits c ON ((d.commit_id = c.id)))
          ORDER BY ss.hostname, ss."timestamp" DESC
        ), latest_available AS (
         SELECT DISTINCT ON (s_1.hostname) s_1.hostname,
            c.id AS latest_commit_id,
            c.git_commit_hash AS latest_commit_hash,
            c.commit_timestamp AS latest_commit_time
           FROM (public.systems s_1
             JOIN public.commits c ON ((s_1.flake_id = c.flake_id)))
          ORDER BY s_1.hostname, c.commit_timestamp DESC
        ), commit_counts AS (
         SELECT cd_1.hostname,
            count(*) AS commits_between
           FROM (((current_deployments cd_1
             JOIN latest_available la_1 ON ((cd_1.hostname = la_1.hostname)))
             JOIN public.systems s_1 ON ((cd_1.hostname = s_1.hostname)))
             JOIN public.commits c ON ((s_1.flake_id = c.flake_id)))
          WHERE ((c.commit_timestamp > cd_1.current_commit_time) AND (c.commit_timestamp <= la_1.latest_commit_time))
          GROUP BY cd_1.hostname
        )
 SELECT COALESCE(cd.hostname, la.hostname, s.hostname) AS hostname,
        CASE
            WHEN (cd.hostname IS NULL) THEN 'Never Deployed'::text
            WHEN (la.latest_commit_id IS NULL) THEN 'No Commits Available'::text
            WHEN (cd.current_commit_id = la.latest_commit_id) THEN 'Up to Date'::text
            WHEN (cd.current_commit_time < la.latest_commit_time) THEN 'Behind'::text
            ELSE 'Unknown'::text
        END AS deployment_status,
    COALESCE(cc.commits_between, (0)::bigint) AS commits_behind,
    "left"(cd.current_commit_hash, 8) AS current_commit,
    "left"(la.latest_commit_hash, 8) AS latest_commit
   FROM (((public.systems s
     LEFT JOIN current_deployments cd ON ((s.hostname = cd.hostname)))
     LEFT JOIN latest_available la ON ((s.hostname = la.hostname)))
     LEFT JOIN commit_counts cc ON ((s.hostname = cc.hostname)))
  WHERE (s.is_active = true)
  ORDER BY
        CASE
            WHEN (cd.hostname IS NULL) THEN 1
            WHEN (la.latest_commit_id IS NULL) THEN 2
            WHEN (cd.current_commit_time < la.latest_commit_time) THEN 3
            WHEN (cd.current_commit_id = la.latest_commit_id) THEN 4
            ELSE 5
        END, COALESCE(cc.commits_between, (0)::bigint) DESC;


ALTER VIEW public.view_deployment_issues OWNER TO crystal_forge;

--
-- Name: view_derivation_stats; Type: VIEW; Schema: public; Owner: crystal_forge
--

CREATE VIEW public.view_derivation_stats AS
SELECT
    NULL::bigint AS total_derivations,
    NULL::bigint AS nixos_systems,
    NULL::bigint AS packages,
    NULL::bigint AS pending,
    NULL::bigint AS queued,
    NULL::bigint AS dry_run_pending,
    NULL::bigint AS dry_run_in_progress,
    NULL::bigint AS dry_run_complete,
    NULL::bigint AS dry_run_failed,
    NULL::bigint AS build_pending,
    NULL::bigint AS build_in_progress,
    NULL::bigint AS build_complete,
    NULL::bigint AS build_failed,
    NULL::bigint AS complete,
    NULL::bigint AS failed,
    NULL::numeric AS success_rate_percent,
    NULL::bigint AS successful_derivations,
    NULL::bigint AS failed_derivations,
    NULL::bigint AS active_derivations,
    NULL::bigint AS derivations_with_paths,
    NULL::bigint AS derivations_without_paths,
    NULL::numeric AS build_completion_percent,
    NULL::numeric AS avg_attempt_count,
    NULL::integer AS max_attempt_count,
    NULL::bigint AS derivations_with_retries,
    NULL::numeric AS retry_rate_percent,
    NULL::numeric AS avg_evaluation_duration_seconds,
    NULL::numeric AS max_evaluation_duration_seconds,
    NULL::bigint AS scheduled_last_hour,
    NULL::bigint AS scheduled_last_day,
    NULL::bigint AS completed_last_hour,
    NULL::bigint AS completed_last_day,
    NULL::bigint AS commit_based_derivations,
    NULL::bigint AS standalone_derivations,
    NULL::timestamp with time zone AS snapshot_timestamp;


ALTER VIEW public.view_derivation_stats OWNER TO crystal_forge;

--
-- Name: VIEW view_derivation_stats; Type: COMMENT; Schema: public; Owner: crystal_forge
--

COMMENT ON VIEW public.view_derivation_stats IS 'Comprehensive statistics on all derivations including status counts, success rates, and performance metrics';


--
-- Name: view_derivation_status_breakdown; Type: VIEW; Schema: public; Owner: crystal_forge
--

CREATE VIEW public.view_derivation_status_breakdown AS
SELECT
    NULL::character varying(50) AS status_name,
    NULL::text AS status_description,
    NULL::boolean AS is_terminal,
    NULL::boolean AS is_success,
    NULL::bigint AS total_count,
    NULL::bigint AS nixos_count,
    NULL::bigint AS package_count,
    NULL::numeric AS avg_attempts,
    NULL::numeric AS avg_duration_seconds,
    NULL::timestamp with time zone AS oldest_scheduled,
    NULL::timestamp with time zone AS newest_scheduled,
    NULL::bigint AS count_last_24h,
    NULL::numeric AS percentage_of_total;


ALTER VIEW public.view_derivation_status_breakdown OWNER TO crystal_forge;

--
-- Name: VIEW view_derivation_status_breakdown; Type: COMMENT; Schema: public; Owner: crystal_forge
--

COMMENT ON VIEW public.view_derivation_status_breakdown IS 'Detailed breakdown of derivations by status with counts and timing metrics';


--
-- Name: view_flake_recent_commits; Type: VIEW; Schema: public; Owner: crystal_forge
--

CREATE VIEW public.view_flake_recent_commits AS
 WITH ranked AS (
         SELECT f.name AS flake,
            c.id AS commit_id,
            c.git_commit_hash AS commit_hash,
            c.commit_timestamp,
            c.attempt_count,
            (EXISTS ( SELECT 1
                   FROM public.derivations d
                  WHERE (d.commit_id = c.id))) AS has_derivations,
            (now() - c.commit_timestamp) AS age,
            (EXTRACT(epoch FROM (now() - c.commit_timestamp)) / 60.0) AS minutes_ago,
            row_number() OVER (PARTITION BY f.name ORDER BY c.commit_timestamp DESC) AS rn
           FROM (public.commits c
             JOIN public.flakes f ON ((f.id = c.flake_id)))
        )
 SELECT flake,
    "left"(commit_hash, 12) AS commit,
    commit_timestamp,
    attempt_count,
        CASE
            WHEN (attempt_count >= 5) THEN '⚠︎ failed/stuck threshold'::text
            WHEN ((attempt_count > 0) AND (NOT has_derivations)) THEN 'retries'::text
            ELSE 'ok'::text
        END AS attempt_status,
    (round(minutes_ago))::integer AS minutes_since_commit,
    age AS age_interval
   FROM ranked
  WHERE (rn <= 3)
  ORDER BY flake, commit_timestamp DESC;


ALTER VIEW public.view_flake_recent_commits OWNER TO crystal_forge;

--
-- Name: view_nixos_derivation_build_queue; Type: VIEW; Schema: public; Owner: crystal_forge
--

CREATE VIEW public.view_nixos_derivation_build_queue AS
 WITH d_filtered AS (
         SELECT derivations.id,
            derivations.commit_id,
            derivations.derivation_type,
            derivations.derivation_name,
            derivations.derivation_path,
            derivations.scheduled_at,
            derivations.completed_at,
            derivations.attempt_count,
            derivations.started_at,
            derivations.evaluation_duration_ms,
            derivations.error_message,
            derivations.pname,
            derivations.version,
            derivations.status_id,
            derivations.derivation_target,
            derivations.build_elapsed_seconds,
            derivations.build_current_target,
            derivations.build_last_activity_seconds,
            derivations.build_last_heartbeat,
            derivations.cf_agent_enabled,
            derivations.store_path
           FROM public.derivations
          WHERE (derivations.status_id = ANY (ARRAY[5, 12]))
        ), roots AS (
         SELECT d.id AS nixos_id,
            c.commit_timestamp AS nixos_commit_ts
           FROM (d_filtered d
             JOIN public.commits c ON ((c.id = d.commit_id)))
          WHERE (d.derivation_type = 'nixos'::text)
        ), pkg_rows AS (
         SELECT p.id,
            p.commit_id,
            p.derivation_type,
            p.derivation_name,
            p.derivation_path,
            p.scheduled_at,
            p.completed_at,
            p.attempt_count,
            p.started_at,
            p.evaluation_duration_ms,
            p.error_message,
            p.pname,
            p.version,
            p.status_id,
            p.derivation_target,
            p.build_elapsed_seconds,
            p.build_current_target,
            p.build_last_activity_seconds,
            p.build_last_heartbeat,
            p.cf_agent_enabled,
            p.store_path,
            r.nixos_id,
            r.nixos_commit_ts,
            0 AS group_order
           FROM ((roots r
             JOIN public.derivation_dependencies dd ON ((dd.derivation_id = r.nixos_id)))
             JOIN d_filtered p ON ((p.id = dd.depends_on_id)))
          WHERE (p.derivation_type = 'package'::text)
        ), nixos_rows AS (
         SELECT n.id,
            n.commit_id,
            n.derivation_type,
            n.derivation_name,
            n.derivation_path,
            n.scheduled_at,
            n.completed_at,
            n.attempt_count,
            n.started_at,
            n.evaluation_duration_ms,
            n.error_message,
            n.pname,
            n.version,
            n.status_id,
            n.derivation_target,
            n.build_elapsed_seconds,
            n.build_current_target,
            n.build_last_activity_seconds,
            n.build_last_heartbeat,
            n.cf_agent_enabled,
            n.store_path,
            r.nixos_id,
            r.nixos_commit_ts,
            1 AS group_order
           FROM (roots r
             JOIN d_filtered n ON ((n.id = r.nixos_id)))
        )
 SELECT id,
    commit_id,
    derivation_type,
    derivation_name,
    derivation_path,
    scheduled_at,
    completed_at,
    attempt_count,
    started_at,
    evaluation_duration_ms,
    error_message,
    pname,
    version,
    status_id,
    derivation_target,
    build_elapsed_seconds,
    build_current_target,
    build_last_activity_seconds,
    build_last_heartbeat,
    cf_agent_enabled,
    store_path
   FROM ( SELECT pkg_rows.id,
            pkg_rows.commit_id,
            pkg_rows.derivation_type,
            pkg_rows.derivation_name,
            pkg_rows.derivation_path,
            pkg_rows.scheduled_at,
            pkg_rows.completed_at,
            pkg_rows.attempt_count,
            pkg_rows.started_at,
            pkg_rows.evaluation_duration_ms,
            pkg_rows.error_message,
            pkg_rows.pname,
            pkg_rows.version,
            pkg_rows.status_id,
            pkg_rows.derivation_target,
            pkg_rows.build_elapsed_seconds,
            pkg_rows.build_current_target,
            pkg_rows.build_last_activity_seconds,
            pkg_rows.build_last_heartbeat,
            pkg_rows.cf_agent_enabled,
            pkg_rows.store_path,
            pkg_rows.nixos_id,
            pkg_rows.nixos_commit_ts,
            pkg_rows.group_order
           FROM pkg_rows
        UNION ALL
         SELECT nixos_rows.id,
            nixos_rows.commit_id,
            nixos_rows.derivation_type,
            nixos_rows.derivation_name,
            nixos_rows.derivation_path,
            nixos_rows.scheduled_at,
            nixos_rows.completed_at,
            nixos_rows.attempt_count,
            nixos_rows.started_at,
            nixos_rows.evaluation_duration_ms,
            nixos_rows.error_message,
            nixos_rows.pname,
            nixos_rows.version,
            nixos_rows.status_id,
            nixos_rows.derivation_target,
            nixos_rows.build_elapsed_seconds,
            nixos_rows.build_current_target,
            nixos_rows.build_last_activity_seconds,
            nixos_rows.build_last_heartbeat,
            nixos_rows.cf_agent_enabled,
            nixos_rows.store_path,
            nixos_rows.nixos_id,
            nixos_rows.nixos_commit_ts,
            nixos_rows.group_order
           FROM nixos_rows) u
  WHERE (attempt_count <= 5)
  ORDER BY nixos_commit_ts DESC, nixos_id, group_order, pname, id;


ALTER VIEW public.view_nixos_derivation_build_queue OWNER TO crystal_forge;

--
-- Name: view_nixos_system_build_progress; Type: VIEW; Schema: public; Owner: crystal_forge
--

CREATE VIEW public.view_nixos_system_build_progress AS
 WITH nixos_systems AS (
         SELECT d.id AS system_id,
            d.derivation_name AS system_name,
            d.commit_id,
            d.attempt_count,
            c.git_commit_hash,
            c.commit_timestamp,
            ds.name AS system_status,
            ds.is_success AS system_complete
           FROM ((public.derivations d
             JOIN public.derivation_statuses ds ON ((d.status_id = ds.id)))
             LEFT JOIN public.commits c ON ((d.commit_id = c.id)))
          WHERE (d.derivation_type = 'nixos'::text)
        ), package_progress AS (
         SELECT ns.system_id,
            ns.system_name,
            ns.commit_id,
            ns.attempt_count,
            ns.git_commit_hash,
            ns.commit_timestamp,
            ns.system_status,
            ns.system_complete,
            count(pkg.id) AS total_packages,
            count(pkg.id) FILTER (WHERE (pkg_ds.is_success = true)) AS completed_packages,
            count(pkg.id) FILTER (WHERE ((pkg_ds.name)::text = ANY ((ARRAY['build-pending'::character varying, 'build-in-progress'::character varying])::text[]))) AS building_packages,
            count(pkg.id) FILTER (WHERE ((pkg_ds.name)::text ~~ '%-failed'::text)) AS failed_packages
           FROM (((nixos_systems ns
             LEFT JOIN public.derivation_dependencies dd ON ((ns.system_id = dd.derivation_id)))
             LEFT JOIN public.derivations pkg ON (((dd.depends_on_id = pkg.id) AND (pkg.derivation_type = 'package'::text))))
             LEFT JOIN public.derivation_statuses pkg_ds ON ((pkg.status_id = pkg_ds.id)))
          GROUP BY ns.system_id, ns.system_name, ns.commit_id, ns.attempt_count, ns.git_commit_hash, ns.commit_timestamp, ns.system_status, ns.system_complete
        )
 SELECT system_name,
    git_commit_hash,
    commit_timestamp,
    system_status,
    total_packages,
    completed_packages,
    building_packages,
    failed_packages,
        CASE
            WHEN (total_packages = 0) THEN 100.0
            ELSE round((((completed_packages)::numeric / (total_packages)::numeric) * 100.0), 1)
        END AS build_progress_percent,
        CASE
            WHEN ((system_complete = true) AND (failed_packages = 0)) THEN 'READY'::text
            WHEN (failed_packages > 0) THEN 'FAILED'::text
            WHEN (((system_status)::text = 'dry-run-failed'::text) AND (attempt_count >= 5)) THEN 'FAILED'::text
            WHEN (building_packages > 0) THEN 'BUILDING'::text
            ELSE 'PENDING'::text
        END AS overall_status
   FROM package_progress
  ORDER BY system_name;


ALTER VIEW public.view_nixos_system_build_progress OWNER TO crystal_forge;

--
-- Name: view_security_trend_30d; Type: VIEW; Schema: public; Owner: crystal_forge
--

CREATE VIEW public.view_security_trend_30d AS
 SELECT snapshot_date,
    total_systems,
    round((((systems_with_tpm)::numeric * 100.0) / (total_systems)::numeric), 1) AS tpm_percentage,
    round((((systems_secure_boot)::numeric * 100.0) / (total_systems)::numeric), 1) AS secure_boot_percentage,
    round((((systems_fips_mode)::numeric * 100.0) / (total_systems)::numeric), 1) AS fips_percentage,
    round((((systems_selinux_enforcing)::numeric * 100.0) / (total_systems)::numeric), 1) AS selinux_percentage,
    unique_agent_versions,
    outdated_agent_count
   FROM public.daily_security_posture
  WHERE (snapshot_date >= (CURRENT_DATE - '30 days'::interval))
  ORDER BY snapshot_date;


ALTER VIEW public.view_security_trend_30d OWNER TO crystal_forge;

--
-- Name: view_system_build_progress; Type: VIEW; Schema: public; Owner: crystal_forge
--

CREATE VIEW public.view_system_build_progress AS
 WITH nixos_systems AS (
         SELECT d.id AS system_derivation_id,
            d.derivation_name AS system_name,
            d.commit_id,
            d.derivation_path AS system_derivation_path,
            ds.name AS system_status,
            ds.is_terminal,
            ds.is_success
           FROM (public.derivations d
             JOIN public.derivation_statuses ds ON ((d.status_id = ds.id)))
          WHERE (d.derivation_type = 'nixos'::text)
        ), component_stats AS (
         SELECT ns.system_derivation_id,
            ns.system_name,
            ns.commit_id,
            ns.system_status,
            ns.is_terminal AS system_is_terminal,
            ns.is_success AS system_is_success,
            count(pkg.id) AS total_components,
            count(pkg.id) FILTER (WHERE ((pkg_ds.name)::text = 'dry-run-complete'::text)) AS components_evaluated,
            count(pkg.id) FILTER (WHERE ((pkg_ds.name)::text = 'build-complete'::text)) AS components_built,
            count(pkg.id) FILTER (WHERE ((pkg_ds.name)::text = ANY ((ARRAY['dry-run-failed'::character varying, 'build-failed'::character varying])::text[]))) AS components_failed,
            count(pkg.id) FILTER (WHERE ((pkg_ds.name)::text = ANY ((ARRAY['build-pending'::character varying, 'build-in-progress'::character varying])::text[]))) AS components_building,
                CASE
                    WHEN (count(pkg.id) = 0) THEN 100.0
                    ELSE round((((count(pkg.id) FILTER (WHERE ((pkg_ds.name)::text = 'build-complete'::text)))::numeric * 100.0) / (count(pkg.id))::numeric), 1)
                END AS build_progress_percent,
                CASE
                    WHEN (((ns.system_status)::text = 'build-complete'::text) AND (count(pkg.id) FILTER (WHERE ((pkg_ds.name)::text <> 'build-complete'::text)) = 0)) THEN 'ready'::text
                    WHEN (count(pkg.id) FILTER (WHERE ((pkg_ds.name)::text = ANY ((ARRAY['dry-run-failed'::character varying, 'build-failed'::character varying])::text[]))) > 0) THEN 'failed'::text
                    WHEN (count(pkg.id) FILTER (WHERE ((pkg_ds.name)::text = ANY ((ARRAY['build-pending'::character varying, 'build-in-progress'::character varying])::text[]))) > 0) THEN 'building'::text
                    ELSE 'evaluating'::text
                END AS overall_status
           FROM (((nixos_systems ns
             LEFT JOIN public.derivation_dependencies dd ON ((ns.system_derivation_id = dd.derivation_id)))
             LEFT JOIN public.derivations pkg ON (((dd.depends_on_id = pkg.id) AND (pkg.derivation_type = 'package'::text))))
             LEFT JOIN public.derivation_statuses pkg_ds ON ((pkg.status_id = pkg_ds.id)))
          GROUP BY ns.system_derivation_id, ns.system_name, ns.commit_id, ns.system_status, ns.is_terminal, ns.is_success
        )
 SELECT system_name,
    commit_id,
    system_status,
    overall_status,
    total_components,
    components_evaluated,
    components_built,
    components_failed,
    components_building,
    build_progress_percent,
    system_is_terminal,
    system_is_success
   FROM component_stats
  ORDER BY system_name;


ALTER VIEW public.view_system_build_progress OWNER TO crystal_forge;

--
-- Name: VIEW view_system_build_progress; Type: COMMENT; Schema: public; Owner: crystal_forge
--

COMMENT ON VIEW public.view_system_build_progress IS 'Aggregates individual derivation build progress to system-level summaries';


--
-- Name: view_system_vulnerability_summary; Type: VIEW; Schema: public; Owner: crystal_forge
--

CREATE VIEW public.view_system_vulnerability_summary AS
 WITH nixos_systems AS (
         SELECT d.id AS system_derivation_id,
            d.derivation_name AS system_name,
            d.commit_id
           FROM public.derivations d
          WHERE (d.derivation_type = 'nixos'::text)
        ), system_packages AS (
         SELECT ns.system_derivation_id,
            ns.system_name,
            ns.commit_id,
            pkg.id AS package_derivation_id,
            pkg.derivation_name AS package_name,
            pkg.pname,
            pkg.version
           FROM ((nixos_systems ns
             LEFT JOIN public.derivation_dependencies dd ON ((ns.system_derivation_id = dd.derivation_id)))
             LEFT JOIN public.derivations pkg ON (((dd.depends_on_id = pkg.id) AND (pkg.derivation_type = 'package'::text))))
        ), vulnerability_stats AS (
         SELECT sp.system_name,
            sp.commit_id,
            count(DISTINCT sp.package_derivation_id) AS total_packages_scanned,
            count(DISTINCT pv.cve_id) AS total_vulnerabilities,
            count(DISTINCT pv.cve_id) FILTER (WHERE (c.cvss_v3_score >= 9.0)) AS critical_count,
            count(DISTINCT pv.cve_id) FILTER (WHERE ((c.cvss_v3_score >= 7.0) AND (c.cvss_v3_score < 9.0))) AS high_count,
            count(DISTINCT pv.cve_id) FILTER (WHERE ((c.cvss_v3_score >= 4.0) AND (c.cvss_v3_score < 7.0))) AS medium_count,
            count(DISTINCT pv.cve_id) FILTER (WHERE ((c.cvss_v3_score < 4.0) AND (c.cvss_v3_score IS NOT NULL))) AS low_count,
            count(DISTINCT pv.cve_id) FILTER (WHERE (c.cvss_v3_score IS NULL)) AS unknown_count,
            count(DISTINCT pv.cve_id) FILTER (WHERE (pv.is_whitelisted = true)) AS whitelisted_count,
                CASE
                    WHEN (count(DISTINCT pv.cve_id) FILTER (WHERE (c.cvss_v3_score >= 9.0)) > 0) THEN 'CRITICAL'::text
                    WHEN (count(DISTINCT pv.cve_id) FILTER (WHERE (c.cvss_v3_score >= 7.0)) > 0) THEN 'HIGH'::text
                    WHEN (count(DISTINCT pv.cve_id) FILTER (WHERE (c.cvss_v3_score >= 4.0)) > 0) THEN 'MEDIUM'::text
                    WHEN (count(DISTINCT pv.cve_id) FILTER (WHERE (c.cvss_v3_score > (0)::numeric)) > 0) THEN 'LOW'::text
                    WHEN (count(DISTINCT pv.cve_id) = 0) THEN 'CLEAN'::text
                    ELSE 'UNKNOWN'::text
                END AS risk_level,
            max(cs.completed_at) AS last_scan_completed
           FROM (((system_packages sp
             LEFT JOIN public.package_vulnerabilities pv ON ((sp.package_derivation_id = pv.derivation_id)))
             LEFT JOIN public.cves c ON (((pv.cve_id)::text = (c.id)::text)))
             LEFT JOIN public.cve_scans cs ON ((sp.system_derivation_id = cs.derivation_id)))
          GROUP BY sp.system_name, sp.commit_id
        )
 SELECT system_name,
    commit_id,
    total_packages_scanned,
    total_vulnerabilities,
    critical_count,
    high_count,
    medium_count,
    low_count,
    unknown_count,
    whitelisted_count,
    risk_level,
    last_scan_completed
   FROM vulnerability_stats
  ORDER BY
        CASE risk_level
            WHEN 'CRITICAL'::text THEN 1
            WHEN 'HIGH'::text THEN 2
            WHEN 'MEDIUM'::text THEN 3
            WHEN 'LOW'::text THEN 4
            WHEN 'CLEAN'::text THEN 5
            ELSE 6
        END, total_vulnerabilities DESC;


ALTER VIEW public.view_system_vulnerability_summary OWNER TO crystal_forge;

--
-- Name: VIEW view_system_vulnerability_summary; Type: COMMENT; Schema: public; Owner: crystal_forge
--

COMMENT ON VIEW public.view_system_vulnerability_summary IS 'Aggregates individual package vulnerabilities to system-level security summaries';


--
-- Name: view_system_deployment_readiness; Type: VIEW; Schema: public; Owner: crystal_forge
--

CREATE VIEW public.view_system_deployment_readiness AS
 WITH deployment_analysis AS (
         SELECT bp.system_name,
            bp.commit_id,
            c.git_commit_hash,
            c.commit_timestamp,
            f.name AS flake_name,
            bp.overall_status AS build_status,
            bp.build_progress_percent,
            bp.total_components,
            bp.components_failed,
            COALESCE(vs.risk_level, 'NOT_SCANNED'::text) AS security_risk_level,
            COALESCE(vs.total_vulnerabilities, (0)::bigint) AS total_vulnerabilities,
            COALESCE(vs.critical_count, (0)::bigint) AS critical_vulnerabilities,
            COALESCE(vs.high_count, (0)::bigint) AS high_vulnerabilities,
            vs.last_scan_completed,
                CASE
                    WHEN (bp.overall_status <> 'ready'::text) THEN 'BUILD_NOT_READY'::text
                    WHEN (COALESCE(vs.risk_level, 'NOT_SCANNED'::text) = 'NOT_SCANNED'::text) THEN 'SCAN_REQUIRED'::text
                    WHEN (COALESCE(vs.critical_count, (0)::bigint) > 0) THEN 'SECURITY_RISK'::text
                    WHEN (COALESCE(vs.high_count, (0)::bigint) > 10) THEN 'HIGH_VULN_COUNT'::text
                    ELSE 'READY'::text
                END AS deployment_readiness,
                CASE
                    WHEN (current_deploy.hostname IS NOT NULL) THEN 'DEPLOYED'::text
                    ELSE 'NOT_DEPLOYED'::text
                END AS deployment_status,
            current_deploy.hostname AS deployed_to_system,
            current_deploy.last_deployed
           FROM ((((public.view_system_build_progress bp
             LEFT JOIN public.commits c ON ((bp.commit_id = c.id)))
             LEFT JOIN public.flakes f ON ((c.flake_id = f.id)))
             LEFT JOIN public.view_system_vulnerability_summary vs ON (((bp.system_name = vs.system_name) AND (bp.commit_id = vs.commit_id))))
             LEFT JOIN ( SELECT DISTINCT ON (s.hostname) s.hostname,
                    d.derivation_name AS system_name,
                    d.commit_id,
                    ss."timestamp" AS last_deployed
                   FROM ((public.systems s
                     JOIN public.derivations d ON (((s.hostname = d.derivation_name) AND (d.derivation_type = 'nixos'::text))))
                     JOIN public.system_states ss ON (((d.derivation_path = ss.derivation_path) AND (s.hostname = ss.hostname))))
                  ORDER BY s.hostname, ss."timestamp" DESC) current_deploy ON (((bp.system_name = current_deploy.system_name) AND (bp.commit_id = current_deploy.commit_id))))
        )
 SELECT system_name,
    commit_id,
    git_commit_hash,
    commit_timestamp,
    flake_name,
    build_status,
    build_progress_percent,
    total_components,
    components_failed,
    security_risk_level,
    total_vulnerabilities,
    critical_vulnerabilities,
    high_vulnerabilities,
    last_scan_completed,
    deployment_readiness,
    deployment_status,
    deployed_to_system,
    last_deployed
   FROM deployment_analysis
  ORDER BY
        CASE deployment_readiness
            WHEN 'READY'::text THEN 1
            WHEN 'HIGH_VULN_COUNT'::text THEN 2
            WHEN 'SECURITY_RISK'::text THEN 3
            WHEN 'SCAN_REQUIRED'::text THEN 4
            WHEN 'BUILD_NOT_READY'::text THEN 5
            ELSE NULL::integer
        END, commit_timestamp DESC;


ALTER VIEW public.view_system_deployment_readiness OWNER TO crystal_forge;

--
-- Name: VIEW view_system_deployment_readiness; Type: COMMENT; Schema: public; Owner: crystal_forge
--

COMMENT ON VIEW public.view_system_deployment_readiness IS 'Combines build and security data to determine system deployment readiness';


--
-- Name: view_system_heartbeat_status; Type: VIEW; Schema: public; Owner: crystal_forge
--

CREATE VIEW public.view_system_heartbeat_status AS
 WITH latest_system_states AS (
         SELECT DISTINCT ON (system_states.hostname) system_states.hostname,
            system_states."timestamp" AS last_state_change,
            system_states.id AS system_state_id
           FROM public.system_states
          ORDER BY system_states.hostname, system_states."timestamp" DESC
        ), latest_heartbeats AS (
         SELECT DISTINCT ON (ss.hostname) ss.hostname,
            ah."timestamp" AS last_heartbeat
           FROM (public.system_states ss
             JOIN public.agent_heartbeats ah ON ((ah.system_state_id = ss.id)))
          ORDER BY ss.hostname, ah."timestamp" DESC
        ), heartbeat_analysis AS (
         SELECT COALESCE(lss.hostname, lhb.hostname) AS hostname,
            lss.last_state_change,
            lhb.last_heartbeat,
            GREATEST(COALESCE(lss.last_state_change, '1970-01-01 00:00:00+00'::timestamp with time zone), COALESCE(lhb.last_heartbeat, '1970-01-01 00:00:00+00'::timestamp with time zone)) AS most_recent_activity,
                CASE
                    WHEN (GREATEST(COALESCE(lss.last_state_change, '1970-01-01 00:00:00+00'::timestamp with time zone), COALESCE(lhb.last_heartbeat, '1970-01-01 00:00:00+00'::timestamp with time zone)) > (now() - '00:15:00'::interval)) THEN 'Healthy'::text
                    WHEN (GREATEST(COALESCE(lss.last_state_change, '1970-01-01 00:00:00+00'::timestamp with time zone), COALESCE(lhb.last_heartbeat, '1970-01-01 00:00:00+00'::timestamp with time zone)) > (now() - '01:00:00'::interval)) THEN 'Warning'::text
                    WHEN (GREATEST(COALESCE(lss.last_state_change, '1970-01-01 00:00:00+00'::timestamp with time zone), COALESCE(lhb.last_heartbeat, '1970-01-01 00:00:00+00'::timestamp with time zone)) > (now() - '04:00:00'::interval)) THEN 'Critical'::text
                    ELSE 'Offline'::text
                END AS heartbeat_status,
            (EXTRACT(epoch FROM (now() - GREATEST(COALESCE(lss.last_state_change, '1970-01-01 00:00:00+00'::timestamp with time zone), COALESCE(lhb.last_heartbeat, '1970-01-01 00:00:00+00'::timestamp with time zone)))) / (60)::numeric) AS minutes_since_last_activity
           FROM (latest_system_states lss
             FULL JOIN latest_heartbeats lhb ON ((lss.hostname = lhb.hostname)))
        )
 SELECT hostname,
    heartbeat_status,
    most_recent_activity,
    last_heartbeat,
    last_state_change,
    round(minutes_since_last_activity, 1) AS minutes_since_last_activity,
        CASE heartbeat_status
            WHEN 'Healthy'::text THEN 'System is active and responding'::text
            WHEN 'Warning'::text THEN 'System may be experiencing issues - no recent activity for 15–60 minutes'::text
            WHEN 'Critical'::text THEN 'No activity for 1–4 hours'::text
            WHEN 'Offline'::text THEN 'No activity for >4 hours'::text
            ELSE NULL::text
        END AS status_description
   FROM heartbeat_analysis
  ORDER BY
        CASE heartbeat_status
            WHEN 'Offline'::text THEN 1
            WHEN 'Critical'::text THEN 2
            WHEN 'Warning'::text THEN 3
            WHEN 'Healthy'::text THEN 4
            ELSE NULL::integer
        END, (round(minutes_since_last_activity, 1)) DESC;


ALTER VIEW public.view_system_heartbeat_status OWNER TO crystal_forge;

--
-- Name: view_velocity_trend_14d; Type: VIEW; Schema: public; Owner: crystal_forge
--

CREATE VIEW public.view_velocity_trend_14d AS
 SELECT snapshot_date,
    new_commits_today,
    commits_evaluated_today,
    commits_deployed_today,
    round(avg_eval_to_deploy_hours, 2) AS avg_eval_to_deploy_hours,
    systems_updated_today,
    round((((commits_deployed_today)::numeric * 100.0) / (NULLIF(commits_evaluated_today, 0))::numeric), 1) AS deployment_rate_percentage
   FROM public.daily_deployment_velocity
  WHERE (snapshot_date >= (CURRENT_DATE - '14 days'::interval))
  ORDER BY snapshot_date;


ALTER VIEW public.view_velocity_trend_14d OWNER TO crystal_forge;

--
-- Name: agent_heartbeats id; Type: DEFAULT; Schema: public; Owner: crystal_forge
--

ALTER TABLE ONLY public.agent_heartbeats ALTER COLUMN id SET DEFAULT nextval('public.agent_heartbeats_id_seq'::regclass);


--
-- Name: build_reservations id; Type: DEFAULT; Schema: public; Owner: crystal_forge
--

ALTER TABLE ONLY public.build_reservations ALTER COLUMN id SET DEFAULT nextval('public.build_reservations_id_seq'::regclass);


--
-- Name: cache_push_jobs id; Type: DEFAULT; Schema: public; Owner: crystal_forge
--

ALTER TABLE ONLY public.cache_push_jobs ALTER COLUMN id SET DEFAULT nextval('public.cache_push_jobs_id_seq'::regclass);


--
-- Name: commits id; Type: DEFAULT; Schema: public; Owner: crystal_forge
--

ALTER TABLE ONLY public.commits ALTER COLUMN id SET DEFAULT nextval('public.tbl_commits_id_seq'::regclass);


--
-- Name: compliance_levels id; Type: DEFAULT; Schema: public; Owner: crystal_forge
--

ALTER TABLE ONLY public.compliance_levels ALTER COLUMN id SET DEFAULT nextval('public.compliance_levels_id_seq'::regclass);


--
-- Name: derivations id; Type: DEFAULT; Schema: public; Owner: crystal_forge
--

ALTER TABLE ONLY public.derivations ALTER COLUMN id SET DEFAULT nextval('public.derivations_id_seq'::regclass);


--
-- Name: flakes id; Type: DEFAULT; Schema: public; Owner: crystal_forge
--

ALTER TABLE ONLY public.flakes ALTER COLUMN id SET DEFAULT nextval('public.tbl_flakes_id_seq'::regclass);


--
-- Name: risk_profiles id; Type: DEFAULT; Schema: public; Owner: crystal_forge
--

ALTER TABLE ONLY public.risk_profiles ALTER COLUMN id SET DEFAULT nextval('public.risk_profiles_id_seq'::regclass);


--
-- Name: system_states id; Type: DEFAULT; Schema: public; Owner: crystal_forge
--

ALTER TABLE ONLY public.system_states ALTER COLUMN id SET DEFAULT nextval('public.tbl_system_states_id_seq'::regclass);


--
-- Name: _sqlx_migrations _sqlx_migrations_pkey; Type: CONSTRAINT; Schema: public; Owner: crystal_forge
--

ALTER TABLE ONLY public._sqlx_migrations
    ADD CONSTRAINT _sqlx_migrations_pkey PRIMARY KEY (version);


--
-- Name: agent_heartbeats agent_heartbeats_pkey; Type: CONSTRAINT; Schema: public; Owner: crystal_forge
--

ALTER TABLE ONLY public.agent_heartbeats
    ADD CONSTRAINT agent_heartbeats_pkey PRIMARY KEY (id);


--
-- Name: build_reservations build_reservations_derivation_unique; Type: CONSTRAINT; Schema: public; Owner: crystal_forge
--

ALTER TABLE ONLY public.build_reservations
    ADD CONSTRAINT build_reservations_derivation_unique UNIQUE (derivation_id);


--
-- Name: build_reservations build_reservations_pkey; Type: CONSTRAINT; Schema: public; Owner: crystal_forge
--

ALTER TABLE ONLY public.build_reservations
    ADD CONSTRAINT build_reservations_pkey PRIMARY KEY (id);


--
-- Name: cache_push_jobs cache_push_jobs_pkey; Type: CONSTRAINT; Schema: public; Owner: crystal_forge
--

ALTER TABLE ONLY public.cache_push_jobs
    ADD CONSTRAINT cache_push_jobs_pkey PRIMARY KEY (id);


--
-- Name: compliance_levels compliance_levels_name_key; Type: CONSTRAINT; Schema: public; Owner: crystal_forge
--

ALTER TABLE ONLY public.compliance_levels
    ADD CONSTRAINT compliance_levels_name_key UNIQUE (name);


--
-- Name: compliance_levels compliance_levels_pkey; Type: CONSTRAINT; Schema: public; Owner: crystal_forge
--

ALTER TABLE ONLY public.compliance_levels
    ADD CONSTRAINT compliance_levels_pkey PRIMARY KEY (id);


--
-- Name: cve_scans cve_scans_pkey; Type: CONSTRAINT; Schema: public; Owner: crystal_forge
--

ALTER TABLE ONLY public.cve_scans
    ADD CONSTRAINT cve_scans_pkey PRIMARY KEY (id);


--
-- Name: cves cves_pkey; Type: CONSTRAINT; Schema: public; Owner: crystal_forge
--

ALTER TABLE ONLY public.cves
    ADD CONSTRAINT cves_pkey PRIMARY KEY (id);


--
-- Name: daily_compliance_snapshots daily_compliance_snapshots_pkey; Type: CONSTRAINT; Schema: public; Owner: crystal_forge
--

ALTER TABLE ONLY public.daily_compliance_snapshots
    ADD CONSTRAINT daily_compliance_snapshots_pkey PRIMARY KEY (snapshot_date);


--
-- Name: daily_deployment_velocity daily_deployment_velocity_pkey; Type: CONSTRAINT; Schema: public; Owner: crystal_forge
--

ALTER TABLE ONLY public.daily_deployment_velocity
    ADD CONSTRAINT daily_deployment_velocity_pkey PRIMARY KEY (snapshot_date);


--
-- Name: daily_drift_snapshots daily_drift_snapshots_pkey; Type: CONSTRAINT; Schema: public; Owner: crystal_forge
--

ALTER TABLE ONLY public.daily_drift_snapshots
    ADD CONSTRAINT daily_drift_snapshots_pkey PRIMARY KEY (snapshot_date, hostname);


--
-- Name: daily_evaluation_health daily_evaluation_health_pkey; Type: CONSTRAINT; Schema: public; Owner: crystal_forge
--

ALTER TABLE ONLY public.daily_evaluation_health
    ADD CONSTRAINT daily_evaluation_health_pkey PRIMARY KEY (snapshot_date);


--
-- Name: daily_heartbeat_health daily_heartbeat_health_pkey; Type: CONSTRAINT; Schema: public; Owner: crystal_forge
--

ALTER TABLE ONLY public.daily_heartbeat_health
    ADD CONSTRAINT daily_heartbeat_health_pkey PRIMARY KEY (snapshot_date);


--
-- Name: daily_security_posture daily_security_posture_pkey; Type: CONSTRAINT; Schema: public; Owner: crystal_forge
--

ALTER TABLE ONLY public.daily_security_posture
    ADD CONSTRAINT daily_security_posture_pkey PRIMARY KEY (snapshot_date);


--
-- Name: derivation_dependencies derivation_dependencies_pkey; Type: CONSTRAINT; Schema: public; Owner: crystal_forge
--

ALTER TABLE ONLY public.derivation_dependencies
    ADD CONSTRAINT derivation_dependencies_pkey PRIMARY KEY (derivation_id, depends_on_id);


--
-- Name: derivation_statuses derivation_statuses_display_order_key; Type: CONSTRAINT; Schema: public; Owner: crystal_forge
--

ALTER TABLE ONLY public.derivation_statuses
    ADD CONSTRAINT derivation_statuses_display_order_key UNIQUE (display_order);


--
-- Name: derivation_statuses derivation_statuses_name_key; Type: CONSTRAINT; Schema: public; Owner: crystal_forge
--

ALTER TABLE ONLY public.derivation_statuses
    ADD CONSTRAINT derivation_statuses_name_key UNIQUE (name);


--
-- Name: derivation_statuses derivation_statuses_pkey; Type: CONSTRAINT; Schema: public; Owner: crystal_forge
--

ALTER TABLE ONLY public.derivation_statuses
    ADD CONSTRAINT derivation_statuses_pkey PRIMARY KEY (id);


--
-- Name: derivations derivations_derivation_path_unique; Type: CONSTRAINT; Schema: public; Owner: crystal_forge
--

ALTER TABLE ONLY public.derivations
    ADD CONSTRAINT derivations_derivation_path_unique UNIQUE (derivation_path);


--
-- Name: environments environments_name_key; Type: CONSTRAINT; Schema: public; Owner: crystal_forge
--

ALTER TABLE ONLY public.environments
    ADD CONSTRAINT environments_name_key UNIQUE (name);


--
-- Name: environments environments_pkey; Type: CONSTRAINT; Schema: public; Owner: crystal_forge
--

ALTER TABLE ONLY public.environments
    ADD CONSTRAINT environments_pkey PRIMARY KEY (id);


--
-- Name: package_vulnerabilities package_vulnerabilities_derivation_id_cve_id_key; Type: CONSTRAINT; Schema: public; Owner: crystal_forge
--

ALTER TABLE ONLY public.package_vulnerabilities
    ADD CONSTRAINT package_vulnerabilities_derivation_id_cve_id_key UNIQUE (derivation_id, cve_id);


--
-- Name: package_vulnerabilities package_vulnerabilities_pkey; Type: CONSTRAINT; Schema: public; Owner: crystal_forge
--

ALTER TABLE ONLY public.package_vulnerabilities
    ADD CONSTRAINT package_vulnerabilities_pkey PRIMARY KEY (id);


--
-- Name: risk_profiles risk_profiles_name_key; Type: CONSTRAINT; Schema: public; Owner: crystal_forge
--

ALTER TABLE ONLY public.risk_profiles
    ADD CONSTRAINT risk_profiles_name_key UNIQUE (name);


--
-- Name: risk_profiles risk_profiles_pkey; Type: CONSTRAINT; Schema: public; Owner: crystal_forge
--

ALTER TABLE ONLY public.risk_profiles
    ADD CONSTRAINT risk_profiles_pkey PRIMARY KEY (id);


--
-- Name: scan_packages scan_packages_pkey; Type: CONSTRAINT; Schema: public; Owner: crystal_forge
--

ALTER TABLE ONLY public.scan_packages
    ADD CONSTRAINT scan_packages_pkey PRIMARY KEY (id);


--
-- Name: scan_packages scan_packages_scan_id_derivation_id_unique; Type: CONSTRAINT; Schema: public; Owner: crystal_forge
--

ALTER TABLE ONLY public.scan_packages
    ADD CONSTRAINT scan_packages_scan_id_derivation_id_unique UNIQUE (scan_id, derivation_id);


--
-- Name: systems systems_hostname_key; Type: CONSTRAINT; Schema: public; Owner: crystal_forge
--

ALTER TABLE ONLY public.systems
    ADD CONSTRAINT systems_hostname_key UNIQUE (hostname);


--
-- Name: systems systems_pkey; Type: CONSTRAINT; Schema: public; Owner: crystal_forge
--

ALTER TABLE ONLY public.systems
    ADD CONSTRAINT systems_pkey PRIMARY KEY (id);


--
-- Name: commits tbl_commits_flake_id_git_commit_hash_key; Type: CONSTRAINT; Schema: public; Owner: crystal_forge
--

ALTER TABLE ONLY public.commits
    ADD CONSTRAINT tbl_commits_flake_id_git_commit_hash_key UNIQUE (flake_id, git_commit_hash);


--
-- Name: commits tbl_commits_pkey; Type: CONSTRAINT; Schema: public; Owner: crystal_forge
--

ALTER TABLE ONLY public.commits
    ADD CONSTRAINT tbl_commits_pkey PRIMARY KEY (id);


--
-- Name: derivations tbl_evaluation_targets_pkey; Type: CONSTRAINT; Schema: public; Owner: crystal_forge
--

ALTER TABLE ONLY public.derivations
    ADD CONSTRAINT tbl_evaluation_targets_pkey PRIMARY KEY (id);


--
-- Name: flakes tbl_flakes_pkey; Type: CONSTRAINT; Schema: public; Owner: crystal_forge
--

ALTER TABLE ONLY public.flakes
    ADD CONSTRAINT tbl_flakes_pkey PRIMARY KEY (id);


--
-- Name: flakes tbl_flakes_repo_url_key; Type: CONSTRAINT; Schema: public; Owner: crystal_forge
--

ALTER TABLE ONLY public.flakes
    ADD CONSTRAINT tbl_flakes_repo_url_key UNIQUE (repo_url);


--
-- Name: system_states tbl_system_states_pkey; Type: CONSTRAINT; Schema: public; Owner: crystal_forge
--

ALTER TABLE ONLY public.system_states
    ADD CONSTRAINT tbl_system_states_pkey PRIMARY KEY (id);


--
-- Name: users users_email_key; Type: CONSTRAINT; Schema: public; Owner: crystal_forge
--

ALTER TABLE ONLY public.users
    ADD CONSTRAINT users_email_key UNIQUE (email);


--
-- Name: users users_pkey; Type: CONSTRAINT; Schema: public; Owner: crystal_forge
--

ALTER TABLE ONLY public.users
    ADD CONSTRAINT users_pkey PRIMARY KEY (id);


--
-- Name: users users_username_key; Type: CONSTRAINT; Schema: public; Owner: crystal_forge
--

ALTER TABLE ONLY public.users
    ADD CONSTRAINT users_username_key UNIQUE (username);


--
-- Name: derivations_commit_name_type_unique; Type: INDEX; Schema: public; Owner: crystal_forge
--

CREATE UNIQUE INDEX derivations_commit_name_type_unique ON public.derivations USING btree (COALESCE(commit_id, '-1'::integer), derivation_name, derivation_type);


--
-- Name: derivations_name_type_unique_null_commit; Type: INDEX; Schema: public; Owner: crystal_forge
--

CREATE UNIQUE INDEX derivations_name_type_unique_null_commit ON public.derivations USING btree (derivation_name, derivation_type) WHERE (commit_id IS NULL);


--
-- Name: idx_agent_heartbeats_system_state_timestamp; Type: INDEX; Schema: public; Owner: crystal_forge
--

CREATE INDEX idx_agent_heartbeats_system_state_timestamp ON public.agent_heartbeats USING btree (system_state_id, "timestamp" DESC);


--
-- Name: idx_build_reservations_heartbeat; Type: INDEX; Schema: public; Owner: crystal_forge
--

CREATE INDEX idx_build_reservations_heartbeat ON public.build_reservations USING btree (heartbeat_at);


--
-- Name: idx_build_reservations_nixos; Type: INDEX; Schema: public; Owner: crystal_forge
--

CREATE INDEX idx_build_reservations_nixos ON public.build_reservations USING btree (nixos_derivation_id);


--
-- Name: idx_build_reservations_worker; Type: INDEX; Schema: public; Owner: crystal_forge
--

CREATE INDEX idx_build_reservations_worker ON public.build_reservations USING btree (worker_id);


--
-- Name: idx_cache_push_jobs_derivation_failed; Type: INDEX; Schema: public; Owner: crystal_forge
--

CREATE INDEX idx_cache_push_jobs_derivation_failed ON public.cache_push_jobs USING btree (derivation_id, status, attempts) WHERE (status = 'failed'::text);


--
-- Name: idx_cache_push_jobs_derivation_id; Type: INDEX; Schema: public; Owner: crystal_forge
--

CREATE INDEX idx_cache_push_jobs_derivation_id ON public.cache_push_jobs USING btree (derivation_id);


--
-- Name: idx_cache_push_jobs_derivation_status; Type: INDEX; Schema: public; Owner: crystal_forge
--

CREATE INDEX idx_cache_push_jobs_derivation_status ON public.cache_push_jobs USING btree (derivation_id, status);


--
-- Name: idx_cache_push_jobs_derivation_status_attempts; Type: INDEX; Schema: public; Owner: crystal_forge
--

CREATE INDEX idx_cache_push_jobs_derivation_status_attempts ON public.cache_push_jobs USING btree (derivation_id, status, attempts);


--
-- Name: idx_cache_push_jobs_derivation_unique; Type: INDEX; Schema: public; Owner: crystal_forge
--

CREATE UNIQUE INDEX idx_cache_push_jobs_derivation_unique ON public.cache_push_jobs USING btree (derivation_id) WHERE (status = ANY (ARRAY['pending'::text, 'in_progress'::text]));


--
-- Name: idx_cache_push_jobs_scheduled_at; Type: INDEX; Schema: public; Owner: crystal_forge
--

CREATE INDEX idx_cache_push_jobs_scheduled_at ON public.cache_push_jobs USING btree (scheduled_at);


--
-- Name: idx_cache_push_jobs_status; Type: INDEX; Schema: public; Owner: crystal_forge
--

CREATE INDEX idx_cache_push_jobs_status ON public.cache_push_jobs USING btree (status);


--
-- Name: idx_cve_scans_completed_at; Type: INDEX; Schema: public; Owner: crystal_forge
--

CREATE INDEX idx_cve_scans_completed_at ON public.cve_scans USING btree (completed_at);


--
-- Name: idx_cve_scans_derivation_id; Type: INDEX; Schema: public; Owner: crystal_forge
--

CREATE INDEX idx_cve_scans_derivation_id ON public.cve_scans USING btree (derivation_id);


--
-- Name: idx_cve_scans_derivation_pending; Type: INDEX; Schema: public; Owner: crystal_forge
--

CREATE INDEX idx_cve_scans_derivation_pending ON public.cve_scans USING btree (derivation_id, status, scheduled_at) WHERE ((status)::text = ANY ((ARRAY['pending'::character varying, 'in_progress'::character varying])::text[]));


--
-- Name: idx_cve_scans_derivation_status; Type: INDEX; Schema: public; Owner: crystal_forge
--

CREATE INDEX idx_cve_scans_derivation_status ON public.cve_scans USING btree (derivation_id, status);


--
-- Name: idx_cve_scans_scanner; Type: INDEX; Schema: public; Owner: crystal_forge
--

CREATE INDEX idx_cve_scans_scanner ON public.cve_scans USING btree (scanner_name);


--
-- Name: idx_cves_cvss_v3_score; Type: INDEX; Schema: public; Owner: crystal_forge
--

CREATE INDEX idx_cves_cvss_v3_score ON public.cves USING btree (cvss_v3_score);


--
-- Name: idx_cves_published; Type: INDEX; Schema: public; Owner: crystal_forge
--

CREATE INDEX idx_cves_published ON public.cves USING btree (published_date);


--
-- Name: idx_d_ready_cve; Type: INDEX; Schema: public; Owner: crystal_forge
--

CREATE INDEX idx_d_ready_cve ON public.derivations USING btree (completed_at, id) WHERE ((derivation_path IS NOT NULL) AND (status_id = ANY (ARRAY[10, 11])));


--
-- Name: idx_derivation_dependencies_depends_on_id; Type: INDEX; Schema: public; Owner: crystal_forge
--

CREATE INDEX idx_derivation_dependencies_depends_on_id ON public.derivation_dependencies USING btree (depends_on_id);


--
-- Name: idx_derivation_dependencies_derivation_id; Type: INDEX; Schema: public; Owner: crystal_forge
--

CREATE INDEX idx_derivation_dependencies_derivation_id ON public.derivation_dependencies USING btree (derivation_id);


--
-- Name: idx_derivations_cf_agent_enabled; Type: INDEX; Schema: public; Owner: crystal_forge
--

CREATE INDEX idx_derivations_cf_agent_enabled ON public.derivations USING btree (cf_agent_enabled);


--
-- Name: idx_derivations_derivation_type; Type: INDEX; Schema: public; Owner: crystal_forge
--

CREATE INDEX idx_derivations_derivation_type ON public.derivations USING btree (derivation_type);


--
-- Name: idx_derivations_pname_version; Type: INDEX; Schema: public; Owner: crystal_forge
--

CREATE INDEX idx_derivations_pname_version ON public.derivations USING btree (pname, version);


--
-- Name: idx_derivations_status_id; Type: INDEX; Schema: public; Owner: crystal_forge
--

CREATE INDEX idx_derivations_status_id ON public.derivations USING btree (status_id);


--
-- Name: idx_derivations_status_path; Type: INDEX; Schema: public; Owner: crystal_forge
--

CREATE INDEX idx_derivations_status_path ON public.derivations USING btree (status_id, derivation_path) WHERE (derivation_path IS NOT NULL);


--
-- Name: idx_derivations_store_path; Type: INDEX; Schema: public; Owner: crystal_forge
--

CREATE INDEX idx_derivations_store_path ON public.derivations USING btree (store_path) WHERE (store_path IS NOT NULL);


--
-- Name: idx_derivations_type_status; Type: INDEX; Schema: public; Owner: crystal_forge
--

CREATE INDEX idx_derivations_type_status ON public.derivations USING btree (derivation_type, status_id);


--
-- Name: idx_environment_name; Type: INDEX; Schema: public; Owner: crystal_forge
--

CREATE INDEX idx_environment_name ON public.environments USING btree (name);


--
-- Name: idx_package_vulnerabilities_cve; Type: INDEX; Schema: public; Owner: crystal_forge
--

CREATE INDEX idx_package_vulnerabilities_cve ON public.package_vulnerabilities USING btree (cve_id);


--
-- Name: idx_package_vulnerabilities_derivation_id; Type: INDEX; Schema: public; Owner: crystal_forge
--

CREATE INDEX idx_package_vulnerabilities_derivation_id ON public.package_vulnerabilities USING btree (derivation_id);


--
-- Name: idx_package_vulnerabilities_whitelist; Type: INDEX; Schema: public; Owner: crystal_forge
--

CREATE INDEX idx_package_vulnerabilities_whitelist ON public.package_vulnerabilities USING btree (is_whitelisted);


--
-- Name: idx_scan_packages_derivation_id; Type: INDEX; Schema: public; Owner: crystal_forge
--

CREATE INDEX idx_scan_packages_derivation_id ON public.scan_packages USING btree (derivation_id);


--
-- Name: idx_scan_packages_scan; Type: INDEX; Schema: public; Owner: crystal_forge
--

CREATE INDEX idx_scan_packages_scan ON public.scan_packages USING btree (scan_id);


--
-- Name: idx_system_states_change_reason; Type: INDEX; Schema: public; Owner: crystal_forge
--

CREATE INDEX idx_system_states_change_reason ON public.system_states USING btree (change_reason);


--
-- Name: idx_systems_deployment_policy; Type: INDEX; Schema: public; Owner: crystal_forge
--

CREATE INDEX idx_systems_deployment_policy ON public.systems USING btree (deployment_policy);


--
-- Name: idx_systems_desired_target; Type: INDEX; Schema: public; Owner: crystal_forge
--

CREATE INDEX idx_systems_desired_target ON public.systems USING btree (desired_target);


--
-- Name: idx_systems_environment_id; Type: INDEX; Schema: public; Owner: crystal_forge
--

CREATE INDEX idx_systems_environment_id ON public.systems USING btree (environment_id);


--
-- Name: idx_systems_hostname; Type: INDEX; Schema: public; Owner: crystal_forge
--

CREATE INDEX idx_systems_hostname ON public.systems USING btree (hostname);


--
-- Name: idx_users_email; Type: INDEX; Schema: public; Owner: crystal_forge
--

CREATE INDEX idx_users_email ON public.users USING btree (email);


--
-- Name: idx_users_type; Type: INDEX; Schema: public; Owner: crystal_forge
--

CREATE INDEX idx_users_type ON public.users USING btree (user_type);


--
-- Name: view_derivation_stats _RETURN; Type: RULE; Schema: public; Owner: crystal_forge
--

CREATE OR REPLACE VIEW public.view_derivation_stats AS
 WITH overall_stats AS (
         SELECT count(*) AS total_derivations,
            count(*) FILTER (WHERE (d.derivation_type = 'nixos'::text)) AS nixos_systems,
            count(*) FILTER (WHERE (d.derivation_type = 'package'::text)) AS packages,
            count(*) FILTER (WHERE ((ds.name)::text = 'pending'::text)) AS pending,
            count(*) FILTER (WHERE ((ds.name)::text = 'queued'::text)) AS queued,
            count(*) FILTER (WHERE ((ds.name)::text = 'dry-run-pending'::text)) AS dry_run_pending,
            count(*) FILTER (WHERE ((ds.name)::text = 'dry-run-in-progress'::text)) AS dry_run_in_progress,
            count(*) FILTER (WHERE ((ds.name)::text = 'dry-run-complete'::text)) AS dry_run_complete,
            count(*) FILTER (WHERE ((ds.name)::text = 'dry-run-failed'::text)) AS dry_run_failed,
            count(*) FILTER (WHERE ((ds.name)::text = 'build-pending'::text)) AS build_pending,
            count(*) FILTER (WHERE ((ds.name)::text = 'build-in-progress'::text)) AS build_in_progress,
            count(*) FILTER (WHERE ((ds.name)::text = 'build-complete'::text)) AS build_complete,
            count(*) FILTER (WHERE ((ds.name)::text = 'build-failed'::text)) AS build_failed,
            count(*) FILTER (WHERE ((ds.name)::text = 'complete'::text)) AS complete,
            count(*) FILTER (WHERE ((ds.name)::text = 'failed'::text)) AS failed,
            count(*) FILTER (WHERE (ds.is_terminal = true)) AS terminal_derivations,
            count(*) FILTER (WHERE (ds.is_terminal = false)) AS active_derivations,
            count(*) FILTER (WHERE ((ds.is_terminal = true) AND (ds.is_success = true))) AS successful_derivations,
            count(*) FILTER (WHERE ((ds.is_terminal = true) AND (ds.is_success = false))) AS failed_derivations,
            count(*) FILTER (WHERE (d.derivation_path IS NOT NULL)) AS derivations_with_paths,
            count(*) FILTER (WHERE (d.derivation_path IS NULL)) AS derivations_without_paths,
            count(*) FILTER (WHERE (d.commit_id IS NOT NULL)) AS derivations_with_commits,
            count(*) FILTER (WHERE (d.commit_id IS NULL)) AS standalone_derivations,
            avg(d.attempt_count) AS avg_attempt_count,
            max(d.attempt_count) AS max_attempt_count,
            count(*) FILTER (WHERE (d.attempt_count > 1)) AS derivations_with_retries,
            avg(d.evaluation_duration_ms) FILTER (WHERE (d.evaluation_duration_ms IS NOT NULL)) AS avg_evaluation_duration_ms,
            max(d.evaluation_duration_ms) AS max_evaluation_duration_ms,
            count(*) FILTER (WHERE (d.scheduled_at >= (now() - '01:00:00'::interval))) AS scheduled_last_hour,
            count(*) FILTER (WHERE (d.scheduled_at >= (now() - '24:00:00'::interval))) AS scheduled_last_day,
            count(*) FILTER (WHERE (d.scheduled_at >= (now() - '7 days'::interval))) AS scheduled_last_week,
            count(*) FILTER (WHERE (d.completed_at >= (now() - '01:00:00'::interval))) AS completed_last_hour,
            count(*) FILTER (WHERE (d.completed_at >= (now() - '24:00:00'::interval))) AS completed_last_day,
            count(*) FILTER (WHERE (d.completed_at >= (now() - '7 days'::interval))) AS completed_last_week
           FROM (public.derivations d
             JOIN public.derivation_statuses ds ON ((d.status_id = ds.id)))
        ), type_breakdown AS (
         SELECT 'nixos'::text AS derivation_type,
            count(*) AS count,
            count(*) FILTER (WHERE ((ds.is_terminal = true) AND (ds.is_success = true))) AS successful,
            count(*) FILTER (WHERE ((ds.is_terminal = true) AND (ds.is_success = false))) AS failed,
            count(*) FILTER (WHERE (ds.is_terminal = false)) AS in_progress,
            avg(d.attempt_count) AS avg_attempts,
            avg(d.evaluation_duration_ms) FILTER (WHERE (d.evaluation_duration_ms IS NOT NULL)) AS avg_duration_ms
           FROM (public.derivations d
             JOIN public.derivation_statuses ds ON ((d.status_id = ds.id)))
          WHERE (d.derivation_type = 'nixos'::text)
        UNION ALL
         SELECT 'package'::text AS derivation_type,
            count(*) AS count,
            count(*) FILTER (WHERE ((ds.is_terminal = true) AND (ds.is_success = true))) AS successful,
            count(*) FILTER (WHERE ((ds.is_terminal = true) AND (ds.is_success = false))) AS failed,
            count(*) FILTER (WHERE (ds.is_terminal = false)) AS in_progress,
            avg(d.attempt_count) AS avg_attempts,
            avg(d.evaluation_duration_ms) FILTER (WHERE (d.evaluation_duration_ms IS NOT NULL)) AS avg_duration_ms
           FROM (public.derivations d
             JOIN public.derivation_statuses ds ON ((d.status_id = ds.id)))
          WHERE (d.derivation_type = 'package'::text)
        ), status_summary AS (
         SELECT ds.name AS status_name,
            ds.description AS status_description,
            ds.is_terminal,
            ds.is_success,
            count(*) AS derivation_count,
            count(*) FILTER (WHERE (d.derivation_type = 'nixos'::text)) AS nixos_count,
            count(*) FILTER (WHERE (d.derivation_type = 'package'::text)) AS package_count,
            avg(d.attempt_count) AS avg_attempts,
            min(d.scheduled_at) AS oldest_scheduled,
            max(d.scheduled_at) AS newest_scheduled
           FROM (public.derivations d
             JOIN public.derivation_statuses ds ON ((d.status_id = ds.id)))
          GROUP BY ds.id, ds.name, ds.description, ds.is_terminal, ds.is_success
          ORDER BY ds.display_order
        )
 SELECT total_derivations,
    nixos_systems,
    packages,
    pending,
    queued,
    dry_run_pending,
    dry_run_in_progress,
    dry_run_complete,
    dry_run_failed,
    build_pending,
    build_in_progress,
    build_complete,
    build_failed,
    complete,
    failed,
    round((((successful_derivations)::numeric / (NULLIF(terminal_derivations, 0))::numeric) * (100)::numeric), 2) AS success_rate_percent,
    successful_derivations,
    failed_derivations,
    active_derivations,
    derivations_with_paths,
    derivations_without_paths,
    round((((derivations_with_paths)::numeric / (NULLIF(total_derivations, 0))::numeric) * (100)::numeric), 2) AS build_completion_percent,
    round(avg_attempt_count, 2) AS avg_attempt_count,
    max_attempt_count,
    derivations_with_retries,
    round((((derivations_with_retries)::numeric / (NULLIF(total_derivations, 0))::numeric) * (100)::numeric), 2) AS retry_rate_percent,
    round((avg_evaluation_duration_ms / 1000.0), 2) AS avg_evaluation_duration_seconds,
    round(((max_evaluation_duration_ms)::numeric / 1000.0), 2) AS max_evaluation_duration_seconds,
    scheduled_last_hour,
    scheduled_last_day,
    completed_last_hour,
    completed_last_day,
    derivations_with_commits AS commit_based_derivations,
    standalone_derivations,
    now() AS snapshot_timestamp
   FROM overall_stats os;


--
-- Name: view_derivation_status_breakdown _RETURN; Type: RULE; Schema: public; Owner: crystal_forge
--

CREATE OR REPLACE VIEW public.view_derivation_status_breakdown AS
 WITH total_derivations AS (
         SELECT count(*) AS total_count
           FROM public.derivations
        )
 SELECT ds.name AS status_name,
    ds.description AS status_description,
    ds.is_terminal,
    ds.is_success,
    count(*) AS total_count,
    count(*) FILTER (WHERE (d.derivation_type = 'nixos'::text)) AS nixos_count,
    count(*) FILTER (WHERE (d.derivation_type = 'package'::text)) AS package_count,
    round(avg(d.attempt_count), 2) AS avg_attempts,
    round((avg(d.evaluation_duration_ms) FILTER (WHERE (d.evaluation_duration_ms IS NOT NULL)) / 1000.0), 2) AS avg_duration_seconds,
    min(d.scheduled_at) AS oldest_scheduled,
    max(d.scheduled_at) AS newest_scheduled,
    count(*) FILTER (WHERE (d.scheduled_at >= (now() - '24:00:00'::interval))) AS count_last_24h,
    round((((count(*))::numeric / (td.total_count)::numeric) * (100)::numeric), 2) AS percentage_of_total
   FROM ((public.derivations d
     JOIN public.derivation_statuses ds ON ((d.status_id = ds.id)))
     CROSS JOIN total_derivations td)
  GROUP BY ds.id, ds.name, ds.description, ds.is_terminal, ds.is_success, td.total_count
  ORDER BY ds.display_order;


--
-- Name: environments trigger_environment_updated_at; Type: TRIGGER; Schema: public; Owner: crystal_forge
--

CREATE TRIGGER trigger_environment_updated_at BEFORE UPDATE ON public.environments FOR EACH ROW EXECUTE FUNCTION public.update_updated_at_column();


--
-- Name: systems trigger_systems_updated_at; Type: TRIGGER; Schema: public; Owner: crystal_forge
--

CREATE TRIGGER trigger_systems_updated_at BEFORE UPDATE ON public.systems FOR EACH ROW EXECUTE FUNCTION public.update_updated_at_column();


--
-- Name: users update_users_updated_at; Type: TRIGGER; Schema: public; Owner: crystal_forge
--

CREATE TRIGGER update_users_updated_at BEFORE UPDATE ON public.users FOR EACH ROW EXECUTE FUNCTION public.update_updated_at_column();


--
-- Name: agent_heartbeats agent_heartbeats_system_state_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: crystal_forge
--

ALTER TABLE ONLY public.agent_heartbeats
    ADD CONSTRAINT agent_heartbeats_system_state_id_fkey FOREIGN KEY (system_state_id) REFERENCES public.system_states(id);


--
-- Name: build_reservations build_reservations_derivation_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: crystal_forge
--

ALTER TABLE ONLY public.build_reservations
    ADD CONSTRAINT build_reservations_derivation_id_fkey FOREIGN KEY (derivation_id) REFERENCES public.derivations(id) ON DELETE CASCADE;


--
-- Name: build_reservations build_reservations_nixos_derivation_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: crystal_forge
--

ALTER TABLE ONLY public.build_reservations
    ADD CONSTRAINT build_reservations_nixos_derivation_id_fkey FOREIGN KEY (nixos_derivation_id) REFERENCES public.derivations(id) ON DELETE CASCADE;


--
-- Name: cache_push_jobs cache_push_jobs_derivation_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: crystal_forge
--

ALTER TABLE ONLY public.cache_push_jobs
    ADD CONSTRAINT cache_push_jobs_derivation_id_fkey FOREIGN KEY (derivation_id) REFERENCES public.derivations(id) ON DELETE CASCADE;


--
-- Name: cve_scans cve_scans_derivation_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: crystal_forge
--

ALTER TABLE ONLY public.cve_scans
    ADD CONSTRAINT cve_scans_derivation_id_fkey FOREIGN KEY (derivation_id) REFERENCES public.derivations(id) ON DELETE CASCADE;


--
-- Name: derivation_dependencies derivation_dependencies_depends_on_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: crystal_forge
--

ALTER TABLE ONLY public.derivation_dependencies
    ADD CONSTRAINT derivation_dependencies_depends_on_id_fkey FOREIGN KEY (depends_on_id) REFERENCES public.derivations(id) ON DELETE CASCADE;


--
-- Name: derivation_dependencies derivation_dependencies_derivation_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: crystal_forge
--

ALTER TABLE ONLY public.derivation_dependencies
    ADD CONSTRAINT derivation_dependencies_derivation_id_fkey FOREIGN KEY (derivation_id) REFERENCES public.derivations(id) ON DELETE CASCADE;


--
-- Name: derivations derivations_status_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: crystal_forge
--

ALTER TABLE ONLY public.derivations
    ADD CONSTRAINT derivations_status_id_fkey FOREIGN KEY (status_id) REFERENCES public.derivation_statuses(id);


--
-- Name: environments environments_compliance_level_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: crystal_forge
--

ALTER TABLE ONLY public.environments
    ADD CONSTRAINT environments_compliance_level_id_fkey FOREIGN KEY (compliance_level_id) REFERENCES public.compliance_levels(id);


--
-- Name: environments environments_risk_profile_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: crystal_forge
--

ALTER TABLE ONLY public.environments
    ADD CONSTRAINT environments_risk_profile_id_fkey FOREIGN KEY (risk_profile_id) REFERENCES public.risk_profiles(id);


--
-- Name: systems fk_systems_desired_target; Type: FK CONSTRAINT; Schema: public; Owner: crystal_forge
--

ALTER TABLE ONLY public.systems
    ADD CONSTRAINT fk_systems_desired_target FOREIGN KEY (desired_target) REFERENCES public.derivations(derivation_path);


--
-- Name: package_vulnerabilities package_vulnerabilities_cve_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: crystal_forge
--

ALTER TABLE ONLY public.package_vulnerabilities
    ADD CONSTRAINT package_vulnerabilities_cve_id_fkey FOREIGN KEY (cve_id) REFERENCES public.cves(id) ON DELETE CASCADE;


--
-- Name: package_vulnerabilities package_vulnerabilities_derivation_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: crystal_forge
--

ALTER TABLE ONLY public.package_vulnerabilities
    ADD CONSTRAINT package_vulnerabilities_derivation_id_fkey FOREIGN KEY (derivation_id) REFERENCES public.derivations(id) ON DELETE CASCADE;


--
-- Name: scan_packages scan_packages_derivation_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: crystal_forge
--

ALTER TABLE ONLY public.scan_packages
    ADD CONSTRAINT scan_packages_derivation_id_fkey FOREIGN KEY (derivation_id) REFERENCES public.derivations(id) ON DELETE CASCADE;


--
-- Name: scan_packages scan_packages_scan_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: crystal_forge
--

ALTER TABLE ONLY public.scan_packages
    ADD CONSTRAINT scan_packages_scan_id_fkey FOREIGN KEY (scan_id) REFERENCES public.cve_scans(id) ON DELETE CASCADE;


--
-- Name: systems systems_environment_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: crystal_forge
--

ALTER TABLE ONLY public.systems
    ADD CONSTRAINT systems_environment_id_fkey FOREIGN KEY (environment_id) REFERENCES public.environments(id);


--
-- Name: systems systems_flake_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: crystal_forge
--

ALTER TABLE ONLY public.systems
    ADD CONSTRAINT systems_flake_id_fkey FOREIGN KEY (flake_id) REFERENCES public.flakes(id);


--
-- Name: commits tbl_commits_flake_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: crystal_forge
--

ALTER TABLE ONLY public.commits
    ADD CONSTRAINT tbl_commits_flake_id_fkey FOREIGN KEY (flake_id) REFERENCES public.flakes(id) ON DELETE CASCADE;


--
-- Name: derivations tbl_evaluation_targets_commit_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: crystal_forge
--

ALTER TABLE ONLY public.derivations
    ADD CONSTRAINT tbl_evaluation_targets_commit_id_fkey FOREIGN KEY (commit_id) REFERENCES public.commits(id) ON DELETE CASCADE;


--
-- Name: SCHEMA public; Type: ACL; Schema: -; Owner: pg_database_owner
--

GRANT USAGE ON SCHEMA public TO grafana;


--
-- Name: TABLE _sqlx_migrations; Type: ACL; Schema: public; Owner: crystal_forge
--

GRANT SELECT ON TABLE public._sqlx_migrations TO grafana;


--
-- Name: TABLE agent_heartbeats; Type: ACL; Schema: public; Owner: crystal_forge
--

GRANT SELECT ON TABLE public.agent_heartbeats TO grafana;


--
-- Name: SEQUENCE agent_heartbeats_id_seq; Type: ACL; Schema: public; Owner: crystal_forge
--

GRANT SELECT ON SEQUENCE public.agent_heartbeats_id_seq TO grafana;


--
-- Name: TABLE build_reservations; Type: ACL; Schema: public; Owner: crystal_forge
--

GRANT SELECT ON TABLE public.build_reservations TO grafana;


--
-- Name: SEQUENCE build_reservations_id_seq; Type: ACL; Schema: public; Owner: crystal_forge
--

GRANT SELECT ON SEQUENCE public.build_reservations_id_seq TO grafana;


--
-- Name: TABLE cache_push_jobs; Type: ACL; Schema: public; Owner: crystal_forge
--

GRANT SELECT ON TABLE public.cache_push_jobs TO grafana;


--
-- Name: SEQUENCE cache_push_jobs_id_seq; Type: ACL; Schema: public; Owner: crystal_forge
--

GRANT SELECT ON SEQUENCE public.cache_push_jobs_id_seq TO grafana;


--
-- Name: TABLE commits; Type: ACL; Schema: public; Owner: crystal_forge
--

GRANT SELECT ON TABLE public.commits TO grafana;


--
-- Name: TABLE compliance_levels; Type: ACL; Schema: public; Owner: crystal_forge
--

GRANT SELECT ON TABLE public.compliance_levels TO grafana;


--
-- Name: SEQUENCE compliance_levels_id_seq; Type: ACL; Schema: public; Owner: crystal_forge
--

GRANT SELECT ON SEQUENCE public.compliance_levels_id_seq TO grafana;


--
-- Name: TABLE cve_scans; Type: ACL; Schema: public; Owner: crystal_forge
--

GRANT SELECT ON TABLE public.cve_scans TO grafana;


--
-- Name: TABLE cves; Type: ACL; Schema: public; Owner: crystal_forge
--

GRANT SELECT ON TABLE public.cves TO grafana;


--
-- Name: TABLE daily_compliance_snapshots; Type: ACL; Schema: public; Owner: crystal_forge
--

GRANT SELECT ON TABLE public.daily_compliance_snapshots TO grafana;


--
-- Name: TABLE daily_deployment_velocity; Type: ACL; Schema: public; Owner: crystal_forge
--

GRANT SELECT ON TABLE public.daily_deployment_velocity TO grafana;


--
-- Name: TABLE daily_drift_snapshots; Type: ACL; Schema: public; Owner: crystal_forge
--

GRANT SELECT ON TABLE public.daily_drift_snapshots TO grafana;


--
-- Name: TABLE daily_evaluation_health; Type: ACL; Schema: public; Owner: crystal_forge
--

GRANT SELECT ON TABLE public.daily_evaluation_health TO grafana;


--
-- Name: TABLE daily_heartbeat_health; Type: ACL; Schema: public; Owner: crystal_forge
--

GRANT SELECT ON TABLE public.daily_heartbeat_health TO grafana;


--
-- Name: TABLE daily_security_posture; Type: ACL; Schema: public; Owner: crystal_forge
--

GRANT SELECT ON TABLE public.daily_security_posture TO grafana;


--
-- Name: TABLE derivation_dependencies; Type: ACL; Schema: public; Owner: crystal_forge
--

GRANT SELECT ON TABLE public.derivation_dependencies TO grafana;


--
-- Name: TABLE derivation_statuses; Type: ACL; Schema: public; Owner: crystal_forge
--

GRANT SELECT ON TABLE public.derivation_statuses TO grafana;


--
-- Name: TABLE derivations; Type: ACL; Schema: public; Owner: crystal_forge
--

GRANT SELECT ON TABLE public.derivations TO grafana;


--
-- Name: SEQUENCE derivations_id_seq; Type: ACL; Schema: public; Owner: crystal_forge
--

GRANT SELECT ON SEQUENCE public.derivations_id_seq TO grafana;


--
-- Name: TABLE derivations_with_status; Type: ACL; Schema: public; Owner: crystal_forge
--

GRANT SELECT ON TABLE public.derivations_with_status TO grafana;


--
-- Name: TABLE environments; Type: ACL; Schema: public; Owner: crystal_forge
--

GRANT SELECT ON TABLE public.environments TO grafana;


--
-- Name: TABLE flakes; Type: ACL; Schema: public; Owner: crystal_forge
--

GRANT SELECT ON TABLE public.flakes TO grafana;


--
-- Name: TABLE package_vulnerabilities; Type: ACL; Schema: public; Owner: crystal_forge
--

GRANT SELECT ON TABLE public.package_vulnerabilities TO grafana;


--
-- Name: TABLE risk_profiles; Type: ACL; Schema: public; Owner: crystal_forge
--

GRANT SELECT ON TABLE public.risk_profiles TO grafana;


--
-- Name: SEQUENCE risk_profiles_id_seq; Type: ACL; Schema: public; Owner: crystal_forge
--

GRANT SELECT ON SEQUENCE public.risk_profiles_id_seq TO grafana;


--
-- Name: TABLE scan_packages; Type: ACL; Schema: public; Owner: crystal_forge
--

GRANT SELECT ON TABLE public.scan_packages TO grafana;


--
-- Name: TABLE system_states; Type: ACL; Schema: public; Owner: crystal_forge
--

GRANT SELECT ON TABLE public.system_states TO grafana;


--
-- Name: TABLE systems; Type: ACL; Schema: public; Owner: crystal_forge
--

GRANT SELECT ON TABLE public.systems TO grafana;


--
-- Name: SEQUENCE tbl_commits_id_seq; Type: ACL; Schema: public; Owner: crystal_forge
--

GRANT SELECT ON SEQUENCE public.tbl_commits_id_seq TO grafana;


--
-- Name: SEQUENCE tbl_flakes_id_seq; Type: ACL; Schema: public; Owner: crystal_forge
--

GRANT SELECT ON SEQUENCE public.tbl_flakes_id_seq TO grafana;


--
-- Name: SEQUENCE tbl_system_states_id_seq; Type: ACL; Schema: public; Owner: crystal_forge
--

GRANT SELECT ON SEQUENCE public.tbl_system_states_id_seq TO grafana;


--
-- Name: TABLE users; Type: ACL; Schema: public; Owner: crystal_forge
--

GRANT SELECT ON TABLE public.users TO grafana;


--
-- Name: TABLE view_build_queue_status; Type: ACL; Schema: public; Owner: crystal_forge
--

GRANT SELECT ON TABLE public.view_build_queue_status TO grafana;


--
-- Name: TABLE view_buildable_derivations; Type: ACL; Schema: public; Owner: crystal_forge
--

GRANT SELECT ON TABLE public.view_buildable_derivations TO grafana;


--
-- Name: TABLE view_commit_build_status; Type: ACL; Schema: public; Owner: crystal_forge
--

GRANT SELECT ON TABLE public.view_commit_build_status TO grafana;


--
-- Name: TABLE view_commit_deployment_timeline; Type: ACL; Schema: public; Owner: crystal_forge
--

GRANT SELECT ON TABLE public.view_commit_deployment_timeline TO grafana;


--
-- Name: TABLE view_commit_nixos_table; Type: ACL; Schema: public; Owner: crystal_forge
--

GRANT SELECT ON TABLE public.view_commit_nixos_table TO grafana;


--
-- Name: TABLE view_commits_stuck_in_evaluation; Type: ACL; Schema: public; Owner: crystal_forge
--

GRANT SELECT ON TABLE public.view_commits_stuck_in_evaluation TO grafana;


--
-- Name: TABLE view_compliance_trend_7d; Type: ACL; Schema: public; Owner: crystal_forge
--

GRANT SELECT ON TABLE public.view_compliance_trend_7d TO grafana;


--
-- Name: TABLE view_system_deployment_status; Type: ACL; Schema: public; Owner: crystal_forge
--

GRANT SELECT ON TABLE public.view_system_deployment_status TO grafana;


--
-- Name: TABLE view_config_timeline; Type: ACL; Schema: public; Owner: crystal_forge
--

GRANT SELECT ON TABLE public.view_config_timeline TO grafana;


--
-- Name: TABLE view_cve_trends_7d; Type: ACL; Schema: public; Owner: crystal_forge
--

GRANT SELECT ON TABLE public.view_cve_trends_7d TO grafana;


--
-- Name: TABLE view_deployment_issues; Type: ACL; Schema: public; Owner: crystal_forge
--

GRANT SELECT ON TABLE public.view_deployment_issues TO grafana;


--
-- Name: TABLE view_derivation_stats; Type: ACL; Schema: public; Owner: crystal_forge
--

GRANT SELECT ON TABLE public.view_derivation_stats TO grafana;


--
-- Name: TABLE view_derivation_status_breakdown; Type: ACL; Schema: public; Owner: crystal_forge
--

GRANT SELECT ON TABLE public.view_derivation_status_breakdown TO grafana;


--
-- Name: TABLE view_flake_recent_commits; Type: ACL; Schema: public; Owner: crystal_forge
--

GRANT SELECT ON TABLE public.view_flake_recent_commits TO grafana;


--
-- Name: TABLE view_nixos_derivation_build_queue; Type: ACL; Schema: public; Owner: crystal_forge
--

GRANT SELECT ON TABLE public.view_nixos_derivation_build_queue TO grafana;


--
-- Name: TABLE view_nixos_system_build_progress; Type: ACL; Schema: public; Owner: crystal_forge
--

GRANT SELECT ON TABLE public.view_nixos_system_build_progress TO grafana;


--
-- Name: TABLE view_security_trend_30d; Type: ACL; Schema: public; Owner: crystal_forge
--

GRANT SELECT ON TABLE public.view_security_trend_30d TO grafana;


--
-- Name: TABLE view_system_build_progress; Type: ACL; Schema: public; Owner: crystal_forge
--

GRANT SELECT ON TABLE public.view_system_build_progress TO grafana;


--
-- Name: TABLE view_system_vulnerability_summary; Type: ACL; Schema: public; Owner: crystal_forge
--

GRANT SELECT ON TABLE public.view_system_vulnerability_summary TO grafana;


--
-- Name: TABLE view_system_deployment_readiness; Type: ACL; Schema: public; Owner: crystal_forge
--

GRANT SELECT ON TABLE public.view_system_deployment_readiness TO grafana;


--
-- Name: TABLE view_system_heartbeat_status; Type: ACL; Schema: public; Owner: crystal_forge
--

GRANT SELECT ON TABLE public.view_system_heartbeat_status TO grafana;


--
-- Name: TABLE view_velocity_trend_14d; Type: ACL; Schema: public; Owner: crystal_forge
--

GRANT SELECT ON TABLE public.view_velocity_trend_14d TO grafana;


--
-- Name: DEFAULT PRIVILEGES FOR SEQUENCES; Type: DEFAULT ACL; Schema: public; Owner: postgres
--

ALTER DEFAULT PRIVILEGES FOR ROLE postgres IN SCHEMA public GRANT SELECT ON SEQUENCES TO grafana;


--
-- Name: DEFAULT PRIVILEGES FOR TABLES; Type: DEFAULT ACL; Schema: public; Owner: postgres
--

ALTER DEFAULT PRIVILEGES FOR ROLE postgres IN SCHEMA public GRANT SELECT ON TABLES TO grafana;


--
-- PostgreSQL database dump complete
--

\unrestrict XRsveR7SRnWfmc3cEPIyEAAbKpICAn3FszXzBLnig098aZYVKaqtrTapIW7trhr


--
-- PostgreSQL database dump
--

-- Dumped from database version 17.5
-- Dumped by pg_dump version 17.5

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
    parent_derivation_id integer,
    pname character varying(255),
    version character varying(100),
    status_id integer NOT NULL,
    CONSTRAINT valid_derivation_type CHECK ((derivation_type = ANY (ARRAY['nixos'::text, 'package'::text])))
);


ALTER TABLE public.derivations OWNER TO crystal_forge;

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
    d.parent_derivation_id,
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
    CONSTRAINT valid_change_reason CHECK ((change_reason = ANY (ARRAY['startup'::text, 'config_change'::text, 'state_delta'::text])))
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
    updated_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP
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
-- Name: idx_cve_scans_completed_at; Type: INDEX; Schema: public; Owner: crystal_forge
--

CREATE INDEX idx_cve_scans_completed_at ON public.cve_scans USING btree (completed_at);


--
-- Name: idx_cve_scans_derivation_id; Type: INDEX; Schema: public; Owner: crystal_forge
--

CREATE INDEX idx_cve_scans_derivation_id ON public.cve_scans USING btree (derivation_id);


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
-- Name: idx_derivations_derivation_type; Type: INDEX; Schema: public; Owner: crystal_forge
--

CREATE INDEX idx_derivations_derivation_type ON public.derivations USING btree (derivation_type);


--
-- Name: idx_derivations_parent_id; Type: INDEX; Schema: public; Owner: crystal_forge
--

CREATE INDEX idx_derivations_parent_id ON public.derivations USING btree (parent_derivation_id);


--
-- Name: idx_derivations_pname_version; Type: INDEX; Schema: public; Owner: crystal_forge
--

CREATE INDEX idx_derivations_pname_version ON public.derivations USING btree (pname, version);


--
-- Name: idx_derivations_status_id; Type: INDEX; Schema: public; Owner: crystal_forge
--

CREATE INDEX idx_derivations_status_id ON public.derivations USING btree (status_id);


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
-- Name: cve_scans cve_scans_derivation_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: crystal_forge
--

ALTER TABLE ONLY public.cve_scans
    ADD CONSTRAINT cve_scans_derivation_id_fkey FOREIGN KEY (derivation_id) REFERENCES public.derivations(id) ON DELETE CASCADE;


--
-- Name: derivations derivations_parent_derivation_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: crystal_forge
--

ALTER TABLE ONLY public.derivations
    ADD CONSTRAINT derivations_parent_derivation_id_fkey FOREIGN KEY (parent_derivation_id) REFERENCES public.derivations(id) ON DELETE CASCADE;


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
-- PostgreSQL database dump complete
--


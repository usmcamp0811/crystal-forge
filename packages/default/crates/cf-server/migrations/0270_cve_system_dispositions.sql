-- Host dispositions override an environment default for one exact finding.
-- OPEN is represented by the absence of an active row.
CREATE TABLE cve_system_dispositions (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    canonical_cve_id varchar(20) NOT NULL REFERENCES cves(id) ON DELETE RESTRICT,
    canonical_package_name text NOT NULL,
    system_id uuid NOT NULL REFERENCES systems(id) ON DELETE RESTRICT,
    state text NOT NULL CHECK (state IN ('accepted', 'scheduled')),
    justification text,
    review_date date,
    accepted_by uuid REFERENCES users(id) ON DELETE RESTRICT,
    accepted_at timestamptz,
    poam_id uuid REFERENCES poams(id) ON DELETE RESTRICT,
    scheduled_by uuid REFERENCES users(id) ON DELETE RESTRICT,
    scheduled_at timestamptz,
    retired_at timestamptz,
    retired_by uuid REFERENCES users(id) ON DELETE RESTRICT,
    retirement_reason text,
    CHECK (canonical_cve_id ~ '^CVE-[0-9]{4}-[0-9]{4,}$'),
    CHECK (canonical_package_name=btrim(canonical_package_name)
        AND canonical_package_name<>''),
    CHECK (
      (state='accepted'
        AND btrim(COALESCE(justification,''))<>''
        AND accepted_by IS NOT NULL AND accepted_at IS NOT NULL
        AND poam_id IS NULL AND scheduled_by IS NULL AND scheduled_at IS NULL)
      OR
      (state='scheduled'
        AND justification IS NULL AND review_date IS NULL
        AND accepted_by IS NULL AND accepted_at IS NULL
        AND poam_id IS NOT NULL
        AND scheduled_by IS NOT NULL AND scheduled_at IS NOT NULL)
    ),
    CHECK ((retired_at IS NULL AND retired_by IS NULL
            AND retirement_reason IS NULL)
        OR (retired_at IS NOT NULL AND retired_by IS NOT NULL
            AND btrim(COALESCE(retirement_reason,''))<>'')),
    UNIQUE (id, canonical_cve_id, canonical_package_name, system_id)
);

CREATE UNIQUE INDEX cve_system_dispositions_one_active
    ON cve_system_dispositions(
      canonical_cve_id,canonical_package_name,system_id)
    WHERE retired_at IS NULL;
CREATE INDEX cve_system_dispositions_history
    ON cve_system_dispositions(
      canonical_cve_id,canonical_package_name,system_id,
      accepted_at DESC,scheduled_at DESC,id DESC);

-- INVARIANT: Host disposition history is append-only. The only permitted
-- update retires an active row, and a retired row cannot change or be deleted.
CREATE FUNCTION protect_cve_system_disposition_history()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP='DELETE' THEN
        RAISE EXCEPTION 'CVE system disposition history is immutable';
    END IF;
    IF TG_OP='INSERT' THEN RETURN NEW; END IF;
    IF OLD.retired_at IS NOT NULL OR NEW.id<>OLD.id
       OR NEW.canonical_cve_id<>OLD.canonical_cve_id
       OR NEW.canonical_package_name<>OLD.canonical_package_name
       OR NEW.system_id<>OLD.system_id
       OR NEW.state<>OLD.state
       OR NEW.justification IS DISTINCT FROM OLD.justification
       OR NEW.review_date IS DISTINCT FROM OLD.review_date
       OR NEW.accepted_by IS DISTINCT FROM OLD.accepted_by
       OR NEW.accepted_at IS DISTINCT FROM OLD.accepted_at
       OR NEW.poam_id IS DISTINCT FROM OLD.poam_id
       OR NEW.scheduled_by IS DISTINCT FROM OLD.scheduled_by
       OR NEW.scheduled_at IS DISTINCT FROM OLD.scheduled_at
       OR NEW.retired_at IS NULL OR NEW.retired_by IS NULL
       OR btrim(COALESCE(NEW.retirement_reason,''))='' THEN
        RAISE EXCEPTION 'A CVE system disposition update must retire the active row';
    END IF;
    RETURN NEW;
END;
$$;
CREATE TRIGGER trigger_protect_cve_system_disposition_history
    BEFORE INSERT OR UPDATE OR DELETE ON cve_system_dispositions
    FOR EACH ROW EXECUTE FUNCTION protect_cve_system_disposition_history();

CREATE VIEW cve_current_system_dispositions AS
SELECT * FROM cve_system_dispositions WHERE retired_at IS NULL;

-- INVARIANT: Environment SCHEDULED owns only current exact subjects without a
-- host override. Host-owned links do not make the environment default stale.
CREATE OR REPLACE FUNCTION cve_coherent_environment_disposition_state(
    v_cve_id text,
    v_package_name text,
    v_environment_id uuid
) RETURNS text LANGUAGE sql STABLE AS $$
SELECT CASE
  WHEN disposition.state='accepted' THEN 'accepted'
  WHEN disposition.state='scheduled'
   AND EXISTS (
     SELECT 1 FROM poams poam
     WHERE poam.id=disposition.poam_id
       AND poam.status<>'completed'
       AND poam.target_date IS NOT NULL
       AND (
         (poam.owner_kind='user'
          AND poam.owner_user_id IS NOT NULL
          AND poam.owner_group_name IS NULL
          AND EXISTS (SELECT 1 FROM users assignee
            WHERE assignee.id=poam.owner_user_id AND assignee.is_active
              AND assignee.user_type='human'))
         OR
         (poam.owner_kind='oidc_group'
          AND poam.owner_user_id IS NULL
          AND poam.owner_group_name IS NOT NULL
          AND btrim(poam.owner_group_name)<>''
          AND EXISTS (SELECT 1 FROM oidc_group_mappings mapping
            WHERE mapping.group_name=poam.owner_group_name))))
   AND NOT EXISTS (
     SELECT 1 FROM view_current_exact_cve_occurrences subject
     WHERE subject.cve_id=v_cve_id
       AND subject.package_name=v_package_name
       AND subject.environment_id=v_environment_id
       AND NOT EXISTS (SELECT 1 FROM cve_current_system_dispositions host
         WHERE host.system_id=subject.system_id
           AND host.canonical_cve_id=v_cve_id
           AND host.canonical_package_name=v_package_name)
       AND NOT EXISTS (
         SELECT 1 FROM poam_cve_finding_links link
         WHERE link.poam_id=disposition.poam_id
           AND link.system_id=subject.system_id
           AND link.canonical_cve_id=v_cve_id
           AND link.canonical_package_name=v_package_name
           AND link.retired_at IS NULL))
   AND NOT EXISTS (
     SELECT 1 FROM poam_cve_finding_links link
     JOIN systems linked_system ON linked_system.id=link.system_id
     WHERE link.poam_id=disposition.poam_id
       AND link.canonical_cve_id=v_cve_id
       AND link.canonical_package_name=v_package_name
       AND link.retired_at IS NULL
       AND linked_system.environment_id=v_environment_id
       AND NOT EXISTS (SELECT 1 FROM cve_current_system_dispositions host
         WHERE host.system_id=link.system_id
           AND host.canonical_cve_id=v_cve_id
           AND host.canonical_package_name=v_package_name)
       AND NOT EXISTS (
         SELECT 1 FROM view_current_exact_cve_occurrences subject
         WHERE subject.system_id=link.system_id
           AND subject.cve_id=v_cve_id
           AND subject.package_name=v_package_name
           AND subject.environment_id=v_environment_id))
    THEN 'scheduled'
  ELSE NULL
END
FROM cve_current_environment_dispositions disposition
WHERE disposition.canonical_cve_id=v_cve_id
  AND disposition.canonical_package_name=v_package_name
  AND disposition.environment_id=v_environment_id
$$;

COMMENT ON TABLE cve_system_dispositions IS
    'Append-only accepted-risk or scheduled-remediation host override for one canonical CVE, package, and system. No active row means no host override.';
COMMENT ON COLUMN cve_system_dispositions.poam_id IS
    'The remediation POA&M for a scheduled host override. Accepted risk never references a POA&M.';
COMMENT ON FUNCTION cve_coherent_environment_disposition_state(text,text,uuid) IS
    'Returns ACCEPTED directly. Returns SCHEDULED only when the active POA&M links are set-equal to current exact subjects without active host overrides.';

-- A host-only generated POA&M can lose its sole active link when the operator
-- clears the host override. Keep its immutable link history without falsely
-- completing or verifying the POA&M. All other live POA&Ms still require an
-- active finding.
CREATE OR REPLACE FUNCTION require_active_poam_finding()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
DECLARE
    v_poam_id uuid;
    v_has_policy boolean;
    v_has_cve boolean;
    v_is_detached_host_only boolean;
    v_status text;
BEGIN
    IF TG_TABLE_NAME = 'poams' THEN
        v_poam_id := COALESCE(
            (to_jsonb(NEW)->>'id')::uuid,
            (to_jsonb(OLD)->>'id')::uuid
        );
    ELSE
        v_poam_id := COALESCE(
            (to_jsonb(NEW)->>'poam_id')::uuid,
            (to_jsonb(OLD)->>'poam_id')::uuid
        );
    END IF;
    SELECT status INTO v_status FROM poams WHERE id = v_poam_id FOR UPDATE;
    IF NOT FOUND THEN RETURN COALESCE(NEW, OLD); END IF;
    SELECT EXISTS (SELECT 1 FROM poam_finding_links
        WHERE poam_id=v_poam_id) INTO v_has_policy;
    SELECT EXISTS (SELECT 1 FROM poam_cve_finding_links
        WHERE poam_id=v_poam_id) INTO v_has_cve;
    IF v_has_policy AND v_has_cve THEN
        RAISE EXCEPTION 'A POA&M cannot mix policy and CVE finding history'
            USING ERRCODE='23514', CONSTRAINT='poams_one_active_finding_type';
    END IF;
    IF v_has_cve AND EXISTS (
        SELECT 1 FROM poam_cve_finding_links first_link
        JOIN poam_cve_finding_links other ON other.poam_id=first_link.poam_id
        WHERE first_link.poam_id=v_poam_id
          AND (other.canonical_cve_id<>first_link.canonical_cve_id
            OR other.canonical_package_name<>first_link.canonical_package_name)
    ) THEN
        RAISE EXCEPTION 'CVE findings in one POA&M must share canonical identity'
            USING ERRCODE='23514', CONSTRAINT='poams_one_cve_identity';
    END IF;
    SELECT EXISTS (SELECT 1 FROM poam_finding_links
        WHERE poam_id=v_poam_id AND retired_at IS NULL) INTO v_has_policy;
    SELECT EXISTS (SELECT 1 FROM poam_cve_finding_links
        WHERE poam_id=v_poam_id AND retired_at IS NULL) INTO v_has_cve;
    SELECT EXISTS (
        SELECT 1
        FROM cve_system_dispositions disposition
        WHERE disposition.poam_id=v_poam_id
          AND disposition.state='scheduled'
          AND disposition.retired_at IS NOT NULL
          AND disposition.retirement_reason IN (
            'host_triage_open','host_triage_changed')
          AND NOT EXISTS (
            SELECT 1 FROM cve_environment_dispositions environment
            WHERE environment.poam_id=v_poam_id)
          AND 1=(
            SELECT COUNT(DISTINCT ROW(
              link.system_id,link.canonical_cve_id,
              link.canonical_package_name))
            FROM poam_cve_finding_links link
            WHERE link.poam_id=v_poam_id)
          AND NOT EXISTS (
            SELECT 1 FROM poam_cve_finding_links link
            WHERE link.poam_id=v_poam_id
              AND (link.system_id<>disposition.system_id
                OR link.canonical_cve_id<>disposition.canonical_cve_id
                OR link.canonical_package_name<>
                   disposition.canonical_package_name))
    ) INTO v_is_detached_host_only;
    IF v_status <> 'completed'
       AND NOT (v_has_policy OR v_has_cve OR v_is_detached_host_only) THEN
        RAISE EXCEPTION 'A non-completed POA&M requires an active finding'
            USING ERRCODE='23514', CONSTRAINT='poams_active_finding_required';
    END IF;
    IF v_status = 'completed' AND (v_has_policy OR v_has_cve) THEN
        RAISE EXCEPTION 'A completed POA&M cannot have an active finding'
            USING ERRCODE='23514', CONSTRAINT='poams_completed_without_active_finding';
    END IF;
    IF TG_OP = 'DELETE' THEN RETURN OLD; END IF;
    RETURN NEW;
END;
$$;

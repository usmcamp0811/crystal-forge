-- INVARIANT: A live environment schedule survives the last member moving out.
-- Only an episode whose entire CVE link history was retired by environment
-- moves may be empty; a scheduled disposition alone cannot justify an empty
-- POA&M. Policy and detached host-only episodes retain their existing rules.
CREATE OR REPLACE FUNCTION require_active_poam_finding()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
DECLARE
    v_poam_id uuid;
    v_has_policy boolean;
    v_has_cve boolean;
    v_is_detached_host_only boolean;
    v_is_moved_environment_only boolean;
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
    SELECT EXISTS (
        SELECT 1 FROM cve_current_environment_dispositions disposition
        JOIN poam_cve_finding_links history
          ON history.poam_id=disposition.poam_id
         AND history.canonical_cve_id=disposition.canonical_cve_id
         AND history.canonical_package_name=disposition.canonical_package_name
        WHERE disposition.poam_id=v_poam_id
          AND disposition.state='scheduled'
          AND history.retired_at IS NOT NULL
          AND history.retirement_reason='environment_moved'
          AND NOT EXISTS (
            SELECT 1 FROM poam_cve_finding_links other
            WHERE other.poam_id=v_poam_id
              AND (other.retired_at IS NULL
                OR other.retirement_reason IS DISTINCT FROM 'environment_moved'
                OR other.canonical_cve_id<>disposition.canonical_cve_id
                OR other.canonical_package_name<>disposition.canonical_package_name))
    ) INTO v_is_moved_environment_only;
    IF v_status <> 'completed'
       AND NOT (v_has_policy OR v_has_cve OR v_is_detached_host_only
                OR (NOT v_has_policy AND v_is_moved_environment_only)) THEN
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

-- INVARIANT: Retiring the schedule without touching its POA&M or links must
-- also recheck the empty-episode exception at commit. A same-transaction
-- replacement schedule or new active link is visible to the deferred guard.
CREATE CONSTRAINT TRIGGER trigger_cve_environment_disposition_require_finding
    AFTER INSERT OR UPDATE OR DELETE ON cve_environment_dispositions
    DEFERRABLE INITIALLY DEFERRED
    FOR EACH ROW EXECUTE FUNCTION require_active_poam_finding();

-- SECURITY: Context systems are current policy/assignment members and active
-- or closure-retired CVE members. A moved-out historical system is not a
-- current context in B merely because its immutable link still belongs to A.
-- An empty A episode has no system context; its disposition grants A scope in
-- poam_visible_to_environments instead.
CREATE OR REPLACE VIEW poam_context_systems AS
SELECT link.poam_id, finding.system_id
FROM poam_current_finding_links link
JOIN poam_findings finding ON finding.id=link.finding_id
UNION
SELECT link.poam_id, link.system_id
FROM poam_current_cve_finding_links link
UNION
SELECT reference.poam_id, system.id
FROM poam_assignment_references reference
JOIN compliance_bundle_assignment_versions version
  ON version.id=reference.assignment_version_id
JOIN compliance_bundle_assignments assignment
  ON assignment.id=version.assignment_id
JOIN systems system ON system.id=assignment.system_id
   OR system.environment_id=assignment.environment_id;

-- SECURITY: A live scheduled environment owns its POA&M even with no current
-- system context. Require every other current context and assignment to be
-- visible; neither retired links nor a moved host's current B scope grants
-- access. All live scheduled scopes must be visible to the caller.
CREATE OR REPLACE FUNCTION poam_visible_to_environments(
    v_poam_id uuid,
    v_environment_ids uuid[]
) RETURNS boolean LANGUAGE sql STABLE AS $$
SELECT (EXISTS (SELECT 1 FROM poam_current_finding_links WHERE poam_id=v_poam_id)
        OR EXISTS (SELECT 1 FROM poam_current_cve_finding_links WHERE poam_id=v_poam_id)
        OR EXISTS (SELECT 1 FROM poam_assignment_references WHERE poam_id=v_poam_id)
        OR EXISTS (
          SELECT 1 FROM cve_current_environment_dispositions disposition
          JOIN poams poam ON poam.id=disposition.poam_id
          JOIN poam_cve_finding_links history
            ON history.poam_id=disposition.poam_id
           AND history.canonical_cve_id=disposition.canonical_cve_id
           AND history.canonical_package_name=disposition.canonical_package_name
          WHERE disposition.poam_id=v_poam_id AND disposition.state='scheduled'
            AND poam.status<>'completed'
            AND disposition.environment_id=ANY(v_environment_ids)
            AND history.retirement_reason='environment_moved'
            AND history.retired_at IS NOT NULL
            AND NOT EXISTS (SELECT 1 FROM poam_finding_links policy
              WHERE policy.poam_id=v_poam_id)
            AND NOT EXISTS (
              SELECT 1 FROM poam_cve_finding_links other
              WHERE other.poam_id=v_poam_id
                AND (other.retired_at IS NULL
                  OR other.retirement_reason IS DISTINCT FROM 'environment_moved'
                  OR other.canonical_cve_id<>disposition.canonical_cve_id
                  OR other.canonical_package_name<>disposition.canonical_package_name))))
  AND NOT EXISTS (
    SELECT 1 FROM poam_context_systems context
    JOIN systems system ON system.id=context.system_id
    WHERE context.poam_id=v_poam_id
      AND (system.environment_id IS NULL
        OR NOT (system.environment_id=ANY(v_environment_ids))))
  AND NOT EXISTS (
    SELECT 1 FROM cve_current_environment_dispositions disposition
    WHERE disposition.poam_id=v_poam_id AND disposition.state='scheduled'
      AND (disposition.environment_id IS NULL
        OR NOT (disposition.environment_id=ANY(v_environment_ids))))
  AND NOT EXISTS (
    SELECT 1 FROM poam_assignment_references reference
    JOIN compliance_bundle_assignment_versions version
      ON version.id=reference.assignment_version_id
    JOIN compliance_bundle_assignments assignment ON assignment.id=version.assignment_id
    LEFT JOIN systems assigned_system ON assigned_system.id=assignment.system_id
    WHERE reference.poam_id=v_poam_id
      AND (COALESCE(assignment.environment_id,assigned_system.environment_id) IS NULL
        OR NOT (COALESCE(assignment.environment_id,assigned_system.environment_id)
            =ANY(v_environment_ids))));
$$;

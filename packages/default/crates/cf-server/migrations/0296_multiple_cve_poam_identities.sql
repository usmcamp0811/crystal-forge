-- INVARIANT: A POA&M may retain multiple exact CVE/package identities, but
-- policy and CVE finding histories remain mutually exclusive. An otherwise
-- empty live CVE episode must account for every historical subject. A host
-- detachment remains valid only for the original single-subject host-only
-- episode; an environment schedule covers only its own canonical pair and
-- requires moved history for that pair. Earlier retirements within that pair
-- may have other reasons, as allowed by 0284.
CREATE OR REPLACE FUNCTION require_active_poam_finding()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
DECLARE
    v_poam_id uuid;
    v_has_policy boolean;
    v_has_cve boolean;
    v_is_accounted_empty_cve boolean;
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
    SELECT EXISTS (SELECT 1 FROM poam_finding_links
        WHERE poam_id=v_poam_id AND retired_at IS NULL) INTO v_has_policy;
    SELECT EXISTS (SELECT 1 FROM poam_cve_finding_links
        WHERE poam_id=v_poam_id AND retired_at IS NULL) INTO v_has_cve;

    IF v_status <> 'completed' AND NOT (v_has_policy OR v_has_cve) THEN
      -- INVARIANT: The NOT EXISTS check is universal over historical links.
      -- A disposition or moved link for one pair cannot justify another pair.
      SELECT EXISTS (SELECT 1 FROM poam_cve_finding_links link
             WHERE link.poam_id=v_poam_id)
       AND NOT EXISTS (
         SELECT 1 FROM poam_cve_finding_links link
         WHERE link.poam_id=v_poam_id
           AND NOT (
             link.retired_at IS NOT NULL
             AND (
               (
                 NOT EXISTS (SELECT 1 FROM cve_environment_dispositions environment
                   WHERE environment.poam_id=v_poam_id)
                 AND 1=(SELECT COUNT(DISTINCT ROW(
                   subject.system_id,subject.canonical_cve_id,
                   subject.canonical_package_name))
                   FROM poam_cve_finding_links subject
                   WHERE subject.poam_id=v_poam_id)
                 AND EXISTS (
                   SELECT 1 FROM cve_system_dispositions host
                   WHERE host.poam_id=v_poam_id
                     AND host.system_id=link.system_id
                     AND host.canonical_cve_id=link.canonical_cve_id
                     AND host.canonical_package_name=link.canonical_package_name
                     AND host.state='scheduled'
                     AND host.retired_at IS NOT NULL
                     AND host.retirement_reason IN (
                       'host_triage_open','host_triage_changed'))
               )
               OR (
                 EXISTS (
                   SELECT 1 FROM cve_current_environment_dispositions environment
                   WHERE environment.poam_id=v_poam_id
                     AND environment.state='scheduled'
                     AND environment.canonical_cve_id=link.canonical_cve_id
                     AND environment.canonical_package_name=link.canonical_package_name)
                 AND EXISTS (
                   SELECT 1 FROM poam_cve_finding_links moved
                   WHERE moved.poam_id=v_poam_id
                     AND moved.canonical_cve_id=link.canonical_cve_id
                     AND moved.canonical_package_name=link.canonical_package_name
                     AND moved.retired_at IS NOT NULL
                     AND moved.retirement_reason='environment_moved')
               )
             )
           )) INTO v_is_accounted_empty_cve;
    END IF;
    IF v_status <> 'completed'
       AND NOT (v_has_policy OR v_has_cve OR v_is_accounted_empty_cve) THEN
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

-- SECURITY: Empty-episode visibility requires moved history for each pair
-- represented by a historical link, with a live schedule for that same pair.
-- The current-context, all-scheduled-scope, and assignment checks from 0284
-- remain unchanged. A retired link does not expose a moved host's new scope.
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
                  OR NOT EXISTS (
                    SELECT 1 FROM cve_current_environment_dispositions scheduled
                    WHERE scheduled.poam_id=v_poam_id
                      AND scheduled.state='scheduled'
                      AND scheduled.canonical_cve_id=other.canonical_cve_id
                      AND scheduled.canonical_package_name=other.canonical_package_name)
                  OR NOT EXISTS (
                    SELECT 1 FROM poam_cve_finding_links moved
                    WHERE moved.poam_id=v_poam_id
                      AND moved.canonical_cve_id=other.canonical_cve_id
                      AND moved.canonical_package_name=other.canonical_package_name
                      AND moved.retired_at IS NOT NULL
                      AND moved.retirement_reason='environment_moved')))))
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

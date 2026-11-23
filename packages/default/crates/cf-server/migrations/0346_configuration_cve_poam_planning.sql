-- Superset of both 0336 and the sibling 0343 allowlist. On a combined
-- rollout, run 0343 first: it replaces the CHECK and does not know poams:create.
ALTER TABLE api_access_token_scopes DROP CONSTRAINT api_access_token_scopes_scope_check;
ALTER TABLE api_access_token_scopes ADD CONSTRAINT api_access_token_scopes_scope_check
    CHECK (scope IN ('cves:read','poams:read','provenance:read','scans:request',
                     'poams:write','poams:verify','poams:close','poams:create',
                     'policies:read','policies:draft','policies:import',
                     'bundles:read','bundles:draft','compliance:preview'));

-- Shared by MCP inventory and POA&M admission. Only position zero in a ready
-- snapshot represents HEAD; a proven execution source can supply its scan.
CREATE FUNCTION configuration_cve_head(v_flake integer, v_name text)
RETURNS TABLE(snapshot_ready boolean, branch text, commit_id integer, commit_hash text,
              configuration_derivation_id integer, execution_derivation_id integer,
              target_ready boolean) LANGUAGE sql STABLE AS $$
    SELECT f.snapshot_ready_at IS NOT NULL,f.branch::text,c.id,c.git_commit_hash::text,
           d.id,d.execution_derivation_id,
           COALESCE(status.name IN ('build-complete','complete')
             AND NULLIF(BTRIM(d.derivation_path),'') IS NOT NULL
             AND NULLIF(BTRIM(d.store_path),'') IS NOT NULL,false)
    FROM flakes f
    LEFT JOIN flake_branch_commit_snapshot h ON h.flake_id=f.id AND h.position=0
      AND f.snapshot_ready_at IS NOT NULL
    LEFT JOIN commits c ON c.id=h.commit_id AND c.flake_id=f.id
    LEFT JOIN view_derivation_execution_evidence d ON d.commit_id=c.id
      AND d.derivation_type='nixos' AND d.derivation_name=v_name
      AND lower(COALESCE(d.policy_results->>'execution_mode','native')) <> 'mock'
    LEFT JOIN derivation_statuses status ON status.id=d.execution_status_id
    WHERE f.id=v_flake
$$;

-- Retains the scan's own provenance. The explicit-scan caller filters by ID;
-- HEAD callers filter by the proven execution ID and use completion ordering.
CREATE VIEW sealed_configuration_cve_scans AS
SELECT s.id AS scan_id,s.completed_at,s.scanner_name,s.scanner_version,
       d.id AS derivation_id,c.id AS commit_id,c.git_commit_hash AS commit_hash,
       d.derivation_path,NULLIF(BTRIM(d.store_path),'') AS scanned_system_store_path,
       c.flake_id,d.derivation_name AS configuration_name,
       jsonb_build_object('total_packages',s.total_packages,
         'total_vulnerabilities',s.total_vulnerabilities,'critical',s.critical_count,
         'high',s.high_count,'medium',s.medium_count,'low',s.low_count) AS raw_counts
FROM cve_scans s JOIN derivations d ON d.id=s.derivation_id
JOIN commits c ON c.id=d.commit_id
WHERE d.derivation_type='nixos'
  AND lower(COALESCE(d.policy_results->>'execution_mode','native')) <> 'mock'
  AND s.status='completed' AND s.completed_at IS NOT NULL AND s.evidence_schema_version=1;

CREATE FUNCTION configuration_head_cve_scan(v_flake integer, v_name text)
RETURNS TABLE(head_commit_id integer, head_commit_hash text,
              configuration_derivation_id integer, scan_id uuid, completed_at timestamptz,
              scanner_name text, scanner_version text, derivation_id integer,
              commit_id integer, commit_hash text, derivation_path text,
              scanned_system_store_path text, raw_counts jsonb) LANGUAGE sql STABLE AS $$
    SELECT head.commit_id,head.commit_hash,head.configuration_derivation_id,
           scan.scan_id,scan.completed_at,scan.scanner_name::text,scan.scanner_version::text,
           scan.derivation_id,scan.commit_id,scan.commit_hash::text,scan.derivation_path::text,
           scan.scanned_system_store_path,scan.raw_counts
    FROM configuration_cve_head(v_flake,v_name) head
    CROSS JOIN LATERAL (
      SELECT scan.* FROM sealed_configuration_cve_scans scan
      WHERE scan.flake_id=v_flake AND scan.configuration_name=v_name
        AND scan.derivation_id=head.execution_derivation_id
      ORDER BY scan.completed_at DESC,scan.scan_id DESC LIMIT 1
    ) scan WHERE head.target_ready
$$;

-- This relationship is never a Current link, even if the scan later becomes
-- authoritative for a running system. Keep both carrier and scan provenance.
CREATE TABLE poam_configuration_cve_findings (
    poam_id uuid PRIMARY KEY REFERENCES poams(id) ON DELETE RESTRICT,
    cve_finding_id uuid NOT NULL UNIQUE,
    system_id uuid NOT NULL,
    canonical_cve_id varchar(20) NOT NULL,
    canonical_package_name text NOT NULL,
    baseline_head_commit_id integer NOT NULL,
    baseline_head_commit_hash text NOT NULL,
    baseline_configuration_derivation_id integer NOT NULL,
    baseline_scan_id uuid NOT NULL,
    baseline_scan_derivation_id integer NOT NULL,
    baseline_scan_commit_id integer NOT NULL,
    baseline_scan_commit_hash text NOT NULL,
    baseline_scan_completed_at timestamptz NOT NULL,
    baseline_scanned_store_path text NOT NULL,
    baseline_occurrence_derivation_path text NOT NULL,
    baseline_observed_package_version text NOT NULL,
    runtime_linked boolean NOT NULL DEFAULT false,
    FOREIGN KEY (cve_finding_id,system_id,canonical_cve_id,canonical_package_name)
        REFERENCES poam_cve_findings(id,system_id,canonical_cve_id,canonical_package_name),
    FOREIGN KEY (baseline_scan_id,baseline_scan_derivation_id,baseline_scan_completed_at)
        REFERENCES cve_scans(id,derivation_id,completed_at) ON DELETE RESTRICT,
    FOREIGN KEY (baseline_scan_id,baseline_occurrence_derivation_path,
                 canonical_cve_id,canonical_package_name)
        REFERENCES cve_scan_vulnerability_observations(scan_id,observed_derivation_path,
                   canonical_cve_id,canonical_package_name) ON DELETE RESTRICT
);
CREATE INDEX poam_configuration_cve_findings_system_idx
    ON poam_configuration_cve_findings(system_id);
CREATE INDEX poam_configuration_cve_findings_baseline_derivation_idx
    ON poam_configuration_cve_findings(baseline_scan_derivation_id);

CREATE FUNCTION protect_poam_configuration_cve_finding()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP = 'DELETE' THEN
        RAISE EXCEPTION 'Configuration planning baselines are immutable';
    ELSIF TG_OP = 'UPDATE' THEN
        IF NOT OLD.runtime_linked AND NEW.runtime_linked
           AND (to_jsonb(OLD) - 'runtime_linked') = (to_jsonb(NEW) - 'runtime_linked') THEN
            -- The deferred finding check requires the same identity to acquire
            -- runtime link history in this transaction.
            RETURN NEW;
        END IF;
        RAISE EXCEPTION 'Configuration planning baselines are immutable';
    ELSIF NEW.runtime_linked THEN
        RAISE EXCEPTION 'A new configuration planning baseline cannot start linked';
    END IF;
    -- Same key order as the authoritative runtime insertion guard (0279).
    PERFORM lock_poam_cve_key(NEW.canonical_cve_id);
    PERFORM lock_poam_finding_key(NEW.system_id,'00000000-0000-0000-0000-000000000000');
    PERFORM lock_poam_cve_finding_key(NEW.system_id,NEW.canonical_cve_id,NEW.canonical_package_name);
    PERFORM 1 FROM poams WHERE id=NEW.poam_id FOR UPDATE;
    IF EXISTS (SELECT 1 FROM poam_cve_finding_links runtime
               WHERE runtime.cve_finding_id=NEW.cve_finding_id AND runtime.retired_at IS NULL) THEN
        RAISE EXCEPTION 'The finding already has active runtime remediation'
          USING ERRCODE='23514', CONSTRAINT='poam_cve_finding_links_one_active_remediation';
    END IF;
    IF EXISTS (
        SELECT 1 FROM view_current_cve_authority authority
        JOIN cve_scan_vulnerability_observations observation ON observation.scan_id=authority.scan_id
        WHERE authority.system_id=NEW.system_id
          AND observation.canonical_cve_id=NEW.canonical_cve_id
          AND observation.canonical_package_name=NEW.canonical_package_name
    ) THEN
        RAISE EXCEPTION 'A current runtime occurrence cannot start a configuration planning POA&M'
          USING ERRCODE='23514', CONSTRAINT='poam_configuration_runtime_available';
    END IF;
    IF NOT EXISTS (
        SELECT 1 FROM systems system
        CROSS JOIN LATERAL configuration_head_cve_scan(system.flake_id,
          COALESCE(NULLIF(BTRIM(system.system_configuration_name),''),system.hostname)) scan
        JOIN cve_scan_vulnerability_observations observation ON observation.scan_id=scan.scan_id
        WHERE system.id=NEW.system_id AND scan.scan_id=NEW.baseline_scan_id
          AND scan.head_commit_id=NEW.baseline_head_commit_id
          AND scan.head_commit_hash=NEW.baseline_head_commit_hash
          AND scan.configuration_derivation_id=NEW.baseline_configuration_derivation_id
          AND scan.commit_id=NEW.baseline_scan_commit_id
          AND scan.commit_hash=NEW.baseline_scan_commit_hash
          AND scan.scanned_system_store_path=NEW.baseline_scanned_store_path
          AND observation.canonical_cve_id=NEW.canonical_cve_id
          AND observation.canonical_package_name=NEW.canonical_package_name
          AND observation.observed_derivation_path=NEW.baseline_occurrence_derivation_path
          AND observation.observed_package_version=NEW.baseline_observed_package_version
          AND NOT observation.is_whitelisted
    ) THEN
        RAISE EXCEPTION 'Planning baseline must match the sealed flake HEAD occurrence'
          USING ERRCODE='23514', CONSTRAINT='poam_configuration_head_baseline';
    END IF;
    RETURN NEW;
END;
$$;
CREATE TRIGGER trigger_protect_poam_configuration_cve_finding
    BEFORE INSERT OR UPDATE OR DELETE ON poam_configuration_cve_findings
    FOR EACH ROW EXECUTE FUNCTION protect_poam_configuration_cve_finding();

-- Run after the runtime provenance guard has taken the CVE/system/exact
-- locks. A pending plan reserves its identity and can take only its own
-- first runtime link, after MCP explicitly sets the handoff marker.
CREATE FUNCTION reject_runtime_link_for_configuration_plan()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM poam_configuration_cve_findings plan
        WHERE (plan.cve_finding_id=NEW.cve_finding_id OR plan.poam_id=NEW.poam_id)
          AND NOT (plan.runtime_linked AND (
              (plan.poam_id=NEW.poam_id AND plan.cve_finding_id=NEW.cve_finding_id)
              OR EXISTS (SELECT 1 FROM poam_cve_finding_links history
                         WHERE history.poam_id=plan.poam_id
                           AND history.cve_finding_id=plan.cve_finding_id)))
    ) THEN
        RAISE EXCEPTION 'The finding already has a configuration planning POA&M'
          USING ERRCODE='23514', CONSTRAINT='poam_configuration_remediation_conflict';
    END IF;
    RETURN NEW;
END;
$$;
CREATE TRIGGER trigger_reject_runtime_link_for_configuration_plan
    BEFORE INSERT ON poam_cve_finding_links
    FOR EACH ROW EXECUTE FUNCTION reject_runtime_link_for_configuration_plan();

-- Retain the existing policy/runtime and moved-member rules from 0296.
CREATE OR REPLACE FUNCTION require_active_poam_finding()
RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE
    v_poam_id uuid;
    v_has_policy boolean;
    v_has_cve boolean;
    v_is_accounted_empty_cve boolean;
    v_status text;
    v_configuration poam_configuration_cve_findings%ROWTYPE;
BEGIN
    IF TG_TABLE_NAME = 'poams' THEN
        v_poam_id := COALESCE((to_jsonb(NEW)->>'id')::uuid,(to_jsonb(OLD)->>'id')::uuid);
    ELSE
        v_poam_id := COALESCE((to_jsonb(NEW)->>'poam_id')::uuid,(to_jsonb(OLD)->>'poam_id')::uuid);
    END IF;
    SELECT status INTO v_status FROM poams WHERE id=v_poam_id FOR UPDATE;
    IF NOT FOUND THEN RETURN COALESCE(NEW,OLD); END IF;
    SELECT EXISTS (SELECT 1 FROM poam_finding_links WHERE poam_id=v_poam_id) INTO v_has_policy;
    SELECT EXISTS (SELECT 1 FROM poam_cve_finding_links WHERE poam_id=v_poam_id) INTO v_has_cve;
    SELECT * INTO v_configuration FROM poam_configuration_cve_findings WHERE poam_id=v_poam_id;
    IF FOUND AND v_configuration.runtime_linked THEN
        IF NOT EXISTS (SELECT 1 FROM poam_cve_finding_links
                       WHERE poam_id=v_poam_id AND cve_finding_id=v_configuration.cve_finding_id) THEN
            RAISE EXCEPTION 'Configuration handoff requires matching runtime link history'
              USING ERRCODE='23514', CONSTRAINT='poam_configuration_runtime_link_required';
        END IF;
    ELSIF FOUND THEN
        IF v_has_policy OR v_has_cve THEN
            RAISE EXCEPTION 'Configuration planning cannot mix with policy or runtime finding history'
              USING ERRCODE='23514', CONSTRAINT='poams_one_active_finding_type';
        END IF;
        IF v_status IN ('awaiting_verification','completed') OR EXISTS (
            SELECT 1 FROM poam_verification_attempts WHERE poam_id=v_poam_id
        ) THEN
            RAISE EXCEPTION 'Configuration planning is not eligible for verification or closure'
              USING ERRCODE='23514', CONSTRAINT='poam_configuration_planning_only';
        END IF;
        RETURN NEW;
    END IF;
    IF v_has_policy AND v_has_cve THEN
        RAISE EXCEPTION 'A POA&M cannot mix policy and CVE finding history'
          USING ERRCODE='23514', CONSTRAINT='poams_one_active_finding_type';
    END IF;
    SELECT EXISTS (SELECT 1 FROM poam_finding_links
        WHERE poam_id=v_poam_id AND retired_at IS NULL) INTO v_has_policy;
    SELECT EXISTS (SELECT 1 FROM poam_cve_finding_links
        WHERE poam_id=v_poam_id AND retired_at IS NULL) INTO v_has_cve;
    IF v_status <> 'completed' AND NOT (v_has_policy OR v_has_cve) THEN
        SELECT EXISTS (SELECT 1 FROM poam_cve_finding_links link WHERE link.poam_id=v_poam_id)
        AND NOT EXISTS (
          SELECT 1 FROM poam_cve_finding_links link WHERE link.poam_id=v_poam_id
          AND NOT (link.retired_at IS NOT NULL AND (
            (NOT EXISTS (SELECT 1 FROM cve_environment_dispositions environment
               WHERE environment.poam_id=v_poam_id)
             AND 1=(SELECT COUNT(DISTINCT ROW(subject.system_id,subject.canonical_cve_id,
                         subject.canonical_package_name)) FROM poam_cve_finding_links subject
                    WHERE subject.poam_id=v_poam_id)
             AND EXISTS (SELECT 1 FROM cve_system_dispositions host
               WHERE host.poam_id=v_poam_id AND host.system_id=link.system_id
                 AND host.canonical_cve_id=link.canonical_cve_id
                 AND host.canonical_package_name=link.canonical_package_name
                 AND host.state='scheduled' AND host.retired_at IS NOT NULL
                 AND host.retirement_reason IN ('host_triage_open','host_triage_changed')))
            OR (EXISTS (SELECT 1 FROM cve_current_environment_dispositions environment
                  WHERE environment.poam_id=v_poam_id AND environment.state='scheduled'
                    AND environment.canonical_cve_id=link.canonical_cve_id
                    AND environment.canonical_package_name=link.canonical_package_name)
                AND EXISTS (SELECT 1 FROM poam_cve_finding_links moved
                  WHERE moved.poam_id=v_poam_id AND moved.canonical_cve_id=link.canonical_cve_id
                    AND moved.canonical_package_name=link.canonical_package_name
                    AND moved.retired_at IS NOT NULL AND moved.retirement_reason='environment_moved'))
          ))) INTO v_is_accounted_empty_cve;
    END IF;
    IF v_status <> 'completed' AND NOT (v_has_policy OR v_has_cve OR v_is_accounted_empty_cve) THEN
        RAISE EXCEPTION 'A non-completed POA&M requires an active finding'
          USING ERRCODE='23514', CONSTRAINT='poams_active_finding_required';
    END IF;
    IF v_status='completed' AND (v_has_policy OR v_has_cve) THEN
        RAISE EXCEPTION 'A completed POA&M cannot have an active finding'
          USING ERRCODE='23514', CONSTRAINT='poams_completed_without_active_finding';
    END IF;
    IF TG_OP='DELETE' THEN RETURN OLD; END IF;
    RETURN NEW;
END;
$$;
CREATE CONSTRAINT TRIGGER trigger_poam_configuration_require_active_finding
    AFTER INSERT OR UPDATE ON poam_configuration_cve_findings DEFERRABLE INITIALLY DEFERRED
    FOR EACH ROW EXECUTE FUNCTION require_active_poam_finding();

CREATE FUNCTION reject_configuration_verification_attempt()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    -- Serialize with a planning link on the same POA&M. The planning guard
    -- also checks existing attempts, whichever insert wins.
    PERFORM 1 FROM poams WHERE id=NEW.poam_id FOR UPDATE;
    IF EXISTS (SELECT 1 FROM poam_configuration_cve_findings
               WHERE poam_id=NEW.poam_id AND NOT runtime_linked) THEN
        RAISE EXCEPTION 'Configuration planning cannot produce a verification attempt'
          USING ERRCODE='23514', CONSTRAINT='poam_configuration_planning_only';
    END IF;
    RETURN NEW;
END;
$$;
CREATE TRIGGER trigger_reject_configuration_verification_attempt
    BEFORE INSERT ON poam_verification_attempts
    FOR EACH ROW EXECUTE FUNCTION reject_configuration_verification_attempt();

CREATE OR REPLACE VIEW poam_context_systems AS
SELECT link.poam_id,finding.system_id FROM poam_current_finding_links link
JOIN poam_findings finding ON finding.id=link.finding_id
UNION SELECT link.poam_id,link.system_id FROM poam_current_cve_finding_links link
UNION SELECT reference.poam_id,system.id FROM poam_assignment_references reference
JOIN compliance_bundle_assignment_versions version ON version.id=reference.assignment_version_id
JOIN compliance_bundle_assignments assignment ON assignment.id=version.assignment_id
JOIN systems system ON system.id=assignment.system_id OR system.environment_id=assignment.environment_id
UNION SELECT poam_id,system_id FROM poam_configuration_cve_findings;

-- Preserve the 0296 visibility checks for every existing finding family.
CREATE OR REPLACE FUNCTION poam_visible_to_environments(v_poam_id uuid,v_environment_ids uuid[])
RETURNS boolean LANGUAGE sql STABLE AS $$
SELECT (EXISTS (SELECT 1 FROM poam_current_finding_links WHERE poam_id=v_poam_id)
        OR EXISTS (SELECT 1 FROM poam_current_cve_finding_links WHERE poam_id=v_poam_id)
        OR EXISTS (SELECT 1 FROM poam_configuration_cve_findings
                   WHERE poam_id=v_poam_id AND NOT runtime_linked)
        OR EXISTS (SELECT 1 FROM poam_assignment_references WHERE poam_id=v_poam_id)
        OR EXISTS (
          SELECT 1 FROM cve_current_environment_dispositions disposition
          JOIN poams poam ON poam.id=disposition.poam_id
          JOIN poam_cve_finding_links history ON history.poam_id=disposition.poam_id
           AND history.canonical_cve_id=disposition.canonical_cve_id
           AND history.canonical_package_name=disposition.canonical_package_name
          WHERE disposition.poam_id=v_poam_id AND disposition.state='scheduled'
            AND poam.status<>'completed' AND disposition.environment_id=ANY(v_environment_ids)
            AND history.retirement_reason='environment_moved' AND history.retired_at IS NOT NULL
            AND NOT EXISTS (SELECT 1 FROM poam_finding_links policy WHERE policy.poam_id=v_poam_id)
            AND NOT EXISTS (
              SELECT 1 FROM poam_cve_finding_links other WHERE other.poam_id=v_poam_id
                AND (other.retired_at IS NULL
                  OR NOT EXISTS (SELECT 1 FROM cve_current_environment_dispositions scheduled
                    WHERE scheduled.poam_id=v_poam_id AND scheduled.state='scheduled'
                      AND scheduled.canonical_cve_id=other.canonical_cve_id
                      AND scheduled.canonical_package_name=other.canonical_package_name)
                  OR NOT EXISTS (SELECT 1 FROM poam_cve_finding_links moved
                    WHERE moved.poam_id=v_poam_id AND moved.canonical_cve_id=other.canonical_cve_id
                      AND moved.canonical_package_name=other.canonical_package_name
                      AND moved.retired_at IS NOT NULL AND moved.retirement_reason='environment_moved')))))
  AND NOT EXISTS (
    SELECT 1 FROM poam_context_systems context JOIN systems system ON system.id=context.system_id
    WHERE context.poam_id=v_poam_id AND (system.environment_id IS NULL
      OR NOT (system.environment_id=ANY(v_environment_ids))))
  AND NOT EXISTS (
    SELECT 1 FROM cve_current_environment_dispositions disposition
    WHERE disposition.poam_id=v_poam_id AND disposition.state='scheduled'
      AND (disposition.environment_id IS NULL OR NOT (disposition.environment_id=ANY(v_environment_ids))))
  AND NOT EXISTS (
    SELECT 1 FROM poam_assignment_references reference
    JOIN compliance_bundle_assignment_versions version ON version.id=reference.assignment_version_id
    JOIN compliance_bundle_assignments assignment ON assignment.id=version.assignment_id
    LEFT JOIN systems assigned_system ON assigned_system.id=assignment.system_id
    WHERE reference.poam_id=v_poam_id
      AND (COALESCE(assignment.environment_id,assigned_system.environment_id) IS NULL
        OR NOT (COALESCE(assignment.environment_id,assigned_system.environment_id)=ANY(v_environment_ids))));
$$;

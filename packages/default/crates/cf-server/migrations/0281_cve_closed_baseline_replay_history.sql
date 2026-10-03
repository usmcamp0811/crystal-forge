-- SECURITY: A matching accepted sealed closure takes precedence over earlier
-- non-closure retirements of the same immutable baseline. Only a restored
-- in-progress POA&M can reuse that closed baseline; other retired history
-- still requires fresh exact Current evidence. The 0279 trigger runs first
-- and retains its CVE, system, then finding lock order.
CREATE OR REPLACE FUNCTION guard_poam_cve_finding_link_replay()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
DECLARE
    v_has_retired_history boolean;
    v_has_closed_history boolean;
BEGIN
    SELECT count(*)>0, COALESCE(bool_or(attempt.id IS NOT NULL),false)
      INTO v_has_retired_history, v_has_closed_history
    FROM poam_cve_finding_links history
    LEFT JOIN poam_verification_attempts attempt
      ON history.retirement_reason='closed:'||attempt.id::text
     AND attempt.poam_id=history.poam_id
     AND attempt.outcome='accepted' AND attempt.sealed_at IS NOT NULL
    WHERE history.poam_id=NEW.poam_id
          AND history.cve_finding_id=NEW.cve_finding_id
          AND history.system_id=NEW.system_id
          AND history.canonical_cve_id=NEW.canonical_cve_id
          AND history.canonical_package_name=NEW.canonical_package_name
          AND history.baseline_scan_id=NEW.baseline_scan_id
          AND history.baseline_scan_derivation_id=NEW.baseline_scan_derivation_id
          AND history.baseline_scan_completed_at=NEW.baseline_scan_completed_at
          AND history.baseline_generation_snapshot_id IS NOT DISTINCT FROM
              NEW.baseline_generation_snapshot_id
          AND history.baseline_generation=NEW.baseline_generation
          AND history.baseline_target_store_path=NEW.baseline_target_store_path
          AND history.baseline_occurrence_derivation_path=
              NEW.baseline_occurrence_derivation_path
          AND history.baseline_observed_package_version=
              NEW.baseline_observed_package_version
          AND history.retired_at IS NOT NULL;
    IF NOT v_has_retired_history THEN RETURN NEW; END IF;

    IF EXISTS (
        SELECT 1 FROM cve_current_system_dispositions host
        WHERE host.system_id=NEW.system_id
          AND host.canonical_cve_id=NEW.canonical_cve_id
          AND host.canonical_package_name=NEW.canonical_package_name
          AND host.state='accepted'
    ) THEN
        RAISE EXCEPTION 'Accepted host risk must be retired before CVE link replay'
            USING ERRCODE='23514',
                  CONSTRAINT='poam_cve_link_replay_accepted_host_risk';
    END IF;

    IF v_has_closed_history AND NOT EXISTS (
        SELECT 1 FROM poams poam
        WHERE poam.id=NEW.poam_id AND poam.status='in_progress'
    ) THEN
        RAISE EXCEPTION 'Closed CVE link replay requires an in-progress POA&M'
            USING ERRCODE='23514',
                  CONSTRAINT='poam_cve_link_replay_in_progress';
    END IF;

    IF NOT v_has_closed_history AND NOT EXISTS (
            SELECT 1 FROM view_current_cve_authority authority
            JOIN cve_scan_vulnerability_observations observation
              ON observation.scan_id=authority.scan_id
             AND observation.observed_derivation_path=
                 NEW.baseline_occurrence_derivation_path
             AND observation.canonical_cve_id=NEW.canonical_cve_id
             AND observation.canonical_package_name=NEW.canonical_package_name
             AND observation.observed_package_version=
                 NEW.baseline_observed_package_version
             AND NOT observation.is_whitelisted
            WHERE authority.system_id=NEW.system_id
              AND authority.generation=NEW.baseline_generation
              AND authority.store_path=NEW.baseline_target_store_path
              AND authority.derivation_id=NEW.baseline_scan_derivation_id
              AND authority.scan_id=NEW.baseline_scan_id
              AND authority.scan_completed_at=NEW.baseline_scan_completed_at
              AND (NEW.baseline_generation_snapshot_id IS NULL
                   OR authority.generation_snapshot_id=
                      NEW.baseline_generation_snapshot_id)
              AND NOT EXISTS (
                  SELECT 1 FROM system_cve_justifications justification
                  WHERE justification.cve_id=NEW.canonical_cve_id
                    AND (justification.system_id IS NULL
                         OR justification.system_id=NEW.system_id))
    ) THEN
        RAISE EXCEPTION 'Retired non-closure CVE link requires current exact baseline'
            USING ERRCODE='23514',
                  CONSTRAINT='poam_cve_link_replay_current_baseline';
    END IF;
    RETURN NEW;
END;
$$;

COMMENT ON FUNCTION guard_poam_cve_finding_link_replay() IS
  'Rejects retired-link replay under active accepted host risk. Identical accepted sealed closure history permits baseline reuse only for an in-progress POA&M, even with earlier non-closure history; without closure history, replay requires the exact authorized Current occurrence.';

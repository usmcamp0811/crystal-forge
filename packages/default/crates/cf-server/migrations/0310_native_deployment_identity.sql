-- TASK-499: bounded PRIMARY deployment identity, independent of Config inventory.
-- No historical row is recertified. No artifact -> derivation/attempt FK is
-- added: historical identity must not introduce an unbounded GC retention edge.
-- Pending and retained bindings retain runnable lineage with their existing FKs.
ALTER TABLE evaluation_snapshots
    DROP CONSTRAINT evaluation_snapshots_schema_version_check,
    DROP CONSTRAINT evaluation_snapshots_integrity_version_check,
    ADD CONSTRAINT evaluation_snapshots_schema_version_check CHECK (schema_version IN (1, 2, 3)),
    ADD CONSTRAINT evaluation_snapshots_integrity_version_check CHECK (integrity_version IN (0, 1, 2, 3)),
    ADD COLUMN deployment_identity jsonb,
    ADD CONSTRAINT evaluation_snapshots_deployment_identity_type CHECK (
        (schema_version = 3 AND deployment_identity IS NOT NULL
         AND jsonb_typeof(deployment_identity) = 'object'
         AND octet_length(deployment_identity::text) <= 16384
         AND lifecycle = 'available' AND error IS NULL
         AND option_count = 0 AND module_count = 0
         AND target_key IS NULL AND source_out_path IS NULL
         AND carrier_drv_path IS NULL AND provenance_state IS NULL
         AND comparison_ready IS NULL AND option_inventory_complete IS NULL
         AND option_inventory_diagnostics IS NULL
         AND option_inventory_diagnostics_truncated IS NULL)
        OR (schema_version <> 3 AND deployment_identity IS NULL)
    );

-- Certification checks the producer's active Native owner and actual persisted
-- result, not the historical presentation sidecar. Malformed input is false.
CREATE FUNCTION evaluation_native_deployment_identity_valid(target_snapshot_id uuid)
RETURNS boolean LANGUAGE plpgsql STABLE STRICT AS $$
DECLARE
    s evaluation_snapshots%ROWTYPE;
    m jsonb;
BEGIN
    SELECT * INTO s FROM evaluation_snapshots WHERE id = target_snapshot_id;
    IF NOT FOUND OR s.schema_version <> 3 THEN RETURN false; END IF;
    m := s.deployment_identity;
    IF m IS NULL OR jsonb_typeof(m) <> 'object'
       OR NOT m ?& ARRAY['attempt_id','attempt_number','commit_sha','derivation_id','drv_path','expected_store_path']
       OR m - ARRAY['attempt_id','attempt_number','commit_sha','derivation_id','drv_path','expected_store_path'] <> '{}'::jsonb
       OR jsonb_typeof(m->'attempt_id') <> 'string'
       OR jsonb_typeof(m->'attempt_number') <> 'number'
       OR jsonb_typeof(m->'derivation_id') <> 'number'
       OR jsonb_typeof(m->'commit_sha') <> 'string'
       OR jsonb_typeof(m->'drv_path') <> 'string'
       OR jsonb_typeof(m->'expected_store_path') <> 'string'
       OR m->>'commit_sha' !~ '^([0-9a-fA-F]{40}|[0-9a-fA-F]{64})$'
       OR m->>'drv_path' !~ '^/nix/store/[^/[:space:]]+\.drv$'
       OR m->>'expected_store_path' !~ '^/nix/store/[^/[:space:]]+$'
       OR (m->>'attempt_number')::integer::text <> m->>'attempt_number'
       OR (m->>'derivation_id')::integer::text <> m->>'derivation_id'
       OR EXISTS (SELECT 1 FROM evaluation_snapshot_options WHERE snapshot_id = s.id)
    THEN RETURN false; END IF;
    RETURN EXISTS (
        SELECT 1 FROM commits c
        JOIN evaluation_attempts a ON a.commit_id = c.id
          AND a.id = (m->>'attempt_id')::uuid
          AND a.attempt_number = (m->>'attempt_number')::integer
          AND a.status = 'in_progress'
        JOIN evaluation_attempt_inventory i ON i.evaluation_attempt_id = a.id
          AND i.commit_id = c.id AND i.attempt_number = a.attempt_number
          AND i.execution_mode = 'native'
          AND s.configuration_name = ANY(i.selected_configuration_names)
        JOIN derivations d ON d.id = (m->>'derivation_id')::integer
          AND d.commit_id = c.id AND d.derivation_name = s.configuration_name
          AND d.derivation_type = 'nixos' AND d.derivation_path = m->>'drv_path'
          AND d.expected_store_path = m->>'expected_store_path'
          AND COALESCE(d.store_path,d.expected_store_path) = m->>'expected_store_path'
          AND d.cf_agent_enabled AND d.policy_requirements_met
          AND d.policy_results->>'evaluation_attempt' = m->>'attempt_number'
          AND NOT COALESCE((d.policy_results->>'evaluation_failure')::boolean, false)
          AND lower(COALESCE(d.policy_results->>'execution_mode', 'native')) <> 'mock'
        WHERE c.id = s.commit_id AND c.git_commit_hash = m->>'commit_sha'
          AND c.evaluation_attempt_count = a.attempt_number
          AND c.evaluation_status = 'in_progress'
          AND NOT COALESCE(c.cancellation_requested, false)
          AND NOT EXISTS (SELECT 1 FROM evaluation_attempts newer
              WHERE newer.commit_id = c.id AND newer.attempt_number > a.attempt_number)
    );
EXCEPTION WHEN OTHERS THEN RETURN false;
END;
$$;

-- Exact live joins remain mandatory after certification. Retained callers use
-- frozen identity; new PRIMARY bindings additionally reject an older attempt.
-- V2 Inspector artifacts never authorize deployment or rollback.
CREATE FUNCTION evaluation_deployment_artifact_matches(
    target_snapshot_id uuid, target_derivation_id integer, current_attempt boolean DEFAULT false
)
RETURNS boolean LANGUAGE sql STABLE AS $$
    SELECT EXISTS (
        SELECT 1 FROM evaluation_snapshots s
        JOIN derivations d ON d.id = target_derivation_id
          AND d.commit_id = s.commit_id AND d.derivation_name = s.configuration_name
          AND d.derivation_type = 'nixos'
        JOIN commits c ON c.id = d.commit_id
        WHERE s.id = target_snapshot_id AND s.lifecycle = 'available'
          AND ((s.schema_version = 1 AND s.integrity_version = 1)
            OR (s.schema_version = 3 AND s.integrity_version = 3
              AND s.deployment_identity->>'commit_sha' = c.git_commit_hash
              AND s.deployment_identity->>'derivation_id' = d.id::text
              AND s.deployment_identity->>'drv_path' = d.derivation_path
              AND s.deployment_identity->>'expected_store_path' = d.expected_store_path
              AND s.deployment_identity->>'expected_store_path' = COALESCE(d.store_path,d.expected_store_path)
              AND (NOT current_attempt OR s.deployment_identity->>'attempt_number' = c.evaluation_attempt_count::text)))
    );
$$;

CREATE OR REPLACE FUNCTION preserve_evaluation_snapshot_artifact()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP = 'INSERT' THEN
        IF NEW.integrity_version <> 0 THEN
            RAISE EXCEPTION 'evaluation snapshot integrity must be certified after insertion';
        END IF;
        RETURN NEW;
    END IF;
    -- Only the derived host metric and a one-time certification may change.
    -- Comparing the remaining row also freezes future identity columns.
    IF (to_jsonb(NEW) - ARRAY['host_delta_count','integrity_version'])
         IS DISTINCT FROM (to_jsonb(OLD) - ARRAY['host_delta_count','integrity_version'])
       OR (NEW.integrity_version IS DISTINCT FROM OLD.integrity_version AND NOT (
           OLD.integrity_version = 0 AND NEW.integrity_version = NEW.schema_version
           AND ((NEW.schema_version IN (1,2) AND evaluation_snapshot_payloads_valid(NEW.id))
             OR (NEW.schema_version = 3 AND evaluation_native_deployment_identity_valid(NEW.id)))
       )) THEN RAISE EXCEPTION 'evaluation snapshot artifacts are immutable'; END IF;
    RETURN NEW;
END;
$$;

CREATE OR REPLACE FUNCTION validate_primary_evaluation_snapshot_selection_v1()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM evaluation_snapshots s WHERE s.id = NEW.current_snapshot_id
          AND s.commit_id = NEW.commit_id AND s.configuration_name = NEW.configuration_name
          AND (s.schema_version = 1 OR (s.schema_version = 3 AND s.integrity_version = 3))
    ) THEN RAISE EXCEPTION 'primary evaluation selector requires exact V1 or certified deployment identity'; END IF;
    RETURN NEW;
END;
$$;

CREATE OR REPLACE FUNCTION enforce_retained_evaluation_artifact()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP = 'UPDATE' THEN RAISE EXCEPTION 'retained evaluation artifacts are immutable'; END IF;
    IF NOT NEW.lineage_verified OR NOT EXISTS (
        SELECT 1 FROM systems system
        JOIN commits c ON c.flake_id = system.flake_id AND c.id = NEW.commit_id
        JOIN derivations d ON d.id = NEW.derivation_id AND d.commit_id = c.id
          AND d.derivation_name = NEW.configuration_name
        JOIN evaluation_snapshots s ON s.id = NEW.snapshot_id
          AND s.commit_id = c.id AND s.configuration_name = d.derivation_name
        WHERE system.id = NEW.system_id
          AND COALESCE(NULLIF(btrim(system.system_configuration_name),''),system.hostname) = NEW.configuration_name
          AND NEW.source_store_path IS NOT NULL AND btrim(NEW.source_store_path) <> ''
          AND NEW.source_store_path = COALESCE(d.store_path,d.expected_store_path)
          AND evaluation_deployment_artifact_matches(s.id,d.id)
    ) THEN RAISE EXCEPTION 'retained generation requires exact certified deployment lineage'; END IF;
    RETURN NEW;
END;
$$;

COMMENT ON COLUMN evaluation_snapshots.deployment_identity IS
    'Schema 3 bounded Native PRIMARY identity: attempt UUID/number, full commit, actual derivation row, .drv and expected output. Not Config inventory or comparison evidence. Frozen without a derivation/attempt FK so unbound history does not pin runnable rows.';
COMMENT ON COLUMN evaluation_snapshots.integrity_version IS
    '0 uncertified; 1 V1 Config inventory; 2 V2 Inspector; 3 bounded Native deployment identity. Types are not interchangeable.';
COMMENT ON TABLE evaluation_snapshot_selections IS
    'Current PRIMARY attempt: V1 or certified schema-3 Native deployment identity. Config Inspector remains independently V2-only. Retained generations never follow this pointer.';

-- Keep external reconciliation's uniqueness, scan, time, and selector rules.
-- Only its artifact-type predicate changes; no path-only authority is added.
CREATE OR REPLACE FUNCTION enforce_external_generation_reconciliation()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF NEW.binding_origin <> 'external_reconciled' THEN RETURN NEW; END IF;
    IF NOT NEW.lineage_verified OR NOT EXISTS (
        WITH latest AS (
            SELECT state.generation,state.store_path,state.timestamp,
                   state.generation_matches_current_store_path
            FROM systems system
            JOIN LATERAL (
                SELECT candidate.generation,candidate.store_path,candidate.timestamp,
                       candidate.generation_matches_current_store_path
                FROM system_states candidate WHERE candidate.hostname=system.hostname
                ORDER BY candidate.timestamp DESC NULLS LAST,candidate.id DESC LIMIT 1
            ) state ON true WHERE system.id=NEW.system_id AND system.is_active
        ), scoped AS (
            SELECT d.id,d.commit_id,d.derivation_name,c.source_archived
            FROM systems system
            JOIN derivations d ON d.derivation_type='nixos'
              AND d.derivation_name=COALESCE(NULLIF(btrim(system.system_configuration_name),''),system.hostname)
            JOIN commits c ON c.id=d.commit_id AND c.flake_id=system.flake_id
            JOIN latest ON COALESCE(d.store_path,d.expected_store_path)=latest.store_path
            WHERE system.id=NEW.system_id
        )
        SELECT 1 FROM latest
        JOIN scoped d ON d.id=NEW.derivation_id AND d.commit_id=NEW.commit_id
          AND d.derivation_name=NEW.configuration_name
        JOIN evaluation_snapshot_selections selection ON selection.commit_id=NEW.commit_id
          AND selection.configuration_name=NEW.configuration_name
          AND selection.current_snapshot_id=NEW.snapshot_id
        JOIN evaluation_snapshots artifact ON artifact.id=selection.current_snapshot_id
          AND evaluation_deployment_artifact_matches(artifact.id,d.id,true)
          AND artifact.completed_at IS NOT NULL AND artifact.completed_at<=latest.timestamp
        WHERE latest.generation=NEW.generation AND latest.store_path=NEW.source_store_path
          AND latest.generation_matches_current_store_path IS TRUE
          AND latest.timestamp IS NOT NULL AND d.source_archived=false
          AND (SELECT COUNT(*) FROM scoped)=1
          AND EXISTS (SELECT 1 FROM cve_scans scan WHERE scan.derivation_id=NEW.derivation_id
              AND scan.status='completed' AND scan.completed_at IS NOT NULL AND scan.evidence_schema_version=1)
    ) THEN
        RAISE EXCEPTION 'external generation requires a unique current observation, certified selected artifact and exact schema-1 scan'
            USING ERRCODE='23514', CONSTRAINT='external_generation_reconciliation_authority';
    END IF;
    RETURN NEW;
END;
$$;

CREATE OR REPLACE VIEW view_active_scheduled_cve_scan_targets AS
WITH ranked_active_deployments AS (
    SELECT deployment.*,row_number() OVER (
        PARTITION BY deployment.system_id ORDER BY deployment.issued_at DESC,deployment.id DESC
    ) AS active_rank
    FROM pending_system_deployments deployment
    WHERE deployment.status='pending' AND deployment.expires_at>now()
)
SELECT system.id AS system_id,system.environment_id,environment.name AS environment_name,
       deployment.id AS deployment_id,derivation.id AS derivation_id,
       commit.git_commit_hash AS commit_hash,scan.id AS scan_id,scan.completed_at
FROM ranked_active_deployments deployment
JOIN systems system ON system.id=deployment.system_id AND system.is_active
JOIN commits commit ON commit.id=deployment.requested_commit_id AND commit.flake_id=system.flake_id
JOIN derivations derivation ON derivation.id=deployment.requested_derivation_id
  AND derivation.commit_id=deployment.requested_commit_id AND derivation.derivation_type='nixos'
  AND derivation.derivation_name=COALESCE(NULLIF(btrim(system.system_configuration_name),''),system.hostname)
  AND COALESCE(derivation.store_path,derivation.expected_store_path)=deployment.target_store_path
JOIN evaluation_snapshots artifact ON artifact.id=deployment.evaluation_snapshot_id
  AND artifact.commit_id=deployment.requested_commit_id
  AND artifact.configuration_name=derivation.derivation_name
  AND evaluation_deployment_artifact_matches(artifact.id,derivation.id)
JOIN LATERAL (
    SELECT candidate.id,candidate.completed_at FROM cve_scans candidate
    WHERE candidate.derivation_id=derivation.id AND candidate.status='completed'
      AND candidate.completed_at IS NOT NULL AND candidate.evidence_schema_version=1
    ORDER BY candidate.completed_at DESC,candidate.id DESC LIMIT 1
) scan ON true
LEFT JOIN environments environment ON environment.id=system.environment_id
WHERE deployment.active_rank=1 AND deployment.evaluation_snapshot_binding_expected;

COMMENT ON VIEW view_active_scheduled_cve_scan_targets IS
    'Latest live deployment with exact immutable certified V1 or Native deployment identity, derivation/output and newest completed schema-1 scan. Ranking precedes validation.';

-- Retained lineage remains supplemental to independent current CVE authority.
CREATE OR REPLACE VIEW view_current_cve_authority AS
WITH latest AS (
    SELECT system.id AS system_id,system.environment_id,system.flake_id,
           COALESCE(NULLIF(btrim(system.system_configuration_name),''),system.hostname) AS configuration_name,
           state.generation,state.store_path,state.generation_matches_current_store_path
    FROM systems system
    JOIN LATERAL (
        SELECT candidate.generation,candidate.store_path,candidate.generation_matches_current_store_path
        FROM system_states candidate WHERE candidate.hostname=system.hostname
        ORDER BY candidate.timestamp DESC NULLS LAST,candidate.id DESC LIMIT 1
    ) state ON true WHERE system.is_active
), valid_observation AS (
    SELECT * FROM latest WHERE generation IS NOT NULL
      AND btrim(COALESCE(store_path,''))<>'' AND generation_matches_current_store_path IS TRUE
), unique_output AS (
    SELECT observation.system_id,observation.environment_id,observation.generation,observation.store_path,
           min(d.id) AS derivation_id,min(d.commit_id) AS commit_id
    FROM valid_observation observation
    JOIN derivations d ON d.derivation_type='nixos' AND d.derivation_name=observation.configuration_name
      AND COALESCE(d.store_path,d.expected_store_path)=observation.store_path
    JOIN commits c ON c.id=d.commit_id AND c.flake_id=observation.flake_id
    GROUP BY observation.system_id,observation.environment_id,observation.generation,observation.store_path
    HAVING count(*)=1
)
SELECT output.system_id,output.environment_id,output.generation,output.store_path,
       output.derivation_id,output.commit_id,scan.id AS scan_id,scan.completed_at AS scan_completed_at,
       retained.id AS generation_snapshot_id,scan.scanner_name,scan.scanner_version
FROM unique_output output
JOIN LATERAL (
    SELECT candidate.id,candidate.completed_at,candidate.scanner_name,candidate.scanner_version
    FROM cve_scans candidate WHERE candidate.derivation_id=output.derivation_id
      AND candidate.status='completed' AND candidate.completed_at IS NOT NULL AND candidate.evidence_schema_version=1
    ORDER BY candidate.completed_at DESC,candidate.id DESC LIMIT 1
) scan ON true
LEFT JOIN evaluation_generation_snapshots retained ON retained.system_id=output.system_id
  AND retained.generation=output.generation AND retained.source_store_path=output.store_path
  AND retained.derivation_id=output.derivation_id AND retained.commit_id=output.commit_id
  AND retained.lineage_verified
  AND evaluation_deployment_artifact_matches(retained.snapshot_id,retained.derivation_id);

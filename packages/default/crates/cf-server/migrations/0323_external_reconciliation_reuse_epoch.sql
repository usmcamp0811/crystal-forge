-- TASK-508: align external generation reconciliation with the two newer
-- authority rules already used by view_current_cve_authority:
--   1. Only observations in the current managed enrollment (0318) qualify.
--   2. Enforced flat derivation aliases (0322) are one physical artifact; a
--      configuration certificate on alias B may use the immutable scan of its
--      proven physical origin A. Distinct drvs with one output stay ambiguous.
-- Every other condition is unchanged from 0310.
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
                FROM view_agent_current_system_states candidate
                WHERE candidate.hostname=system.hostname
                ORDER BY candidate.timestamp DESC NULLS LAST,candidate.id DESC LIMIT 1
            ) state ON true WHERE system.id=NEW.system_id AND system.is_active
        ), scoped AS (
            SELECT d.id,d.commit_id,d.derivation_name,c.source_archived,
                   COALESCE(d.reused_from_derivation_id,d.id) AS physical_id
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
          AND (SELECT COUNT(DISTINCT physical_id) FROM scoped)=1
          AND EXISTS (SELECT 1 FROM cve_scans scan
              WHERE (scan.derivation_id=NEW.derivation_id
                     OR derivations_share_evidence_scope(NEW.derivation_id,scan.derivation_id))
                AND scan.status='completed' AND scan.completed_at IS NOT NULL
                AND scan.evidence_schema_version=1)
    ) THEN
        RAISE EXCEPTION 'external generation requires a unique current observation, certified selected artifact and exact schema-1 scan'
            USING ERRCODE='23514', CONSTRAINT='external_generation_reconciliation_authority';
    END IF;
    RETURN NEW;
END;
$$;

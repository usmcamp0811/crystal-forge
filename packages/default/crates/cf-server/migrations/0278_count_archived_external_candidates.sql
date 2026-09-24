-- All scoped derivations count toward uniqueness, including archived ones.
-- An archived sole match remains provisional, but it also prevents an active
-- same-output derivation from silently gaining authority over an ambiguous
-- running target. Replace the function; migration 0277 may already be applied.
CREATE OR REPLACE FUNCTION enforce_external_generation_reconciliation()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF NEW.binding_origin <> 'external_reconciled' THEN
        RETURN NEW;
    END IF;
    IF NOT NEW.lineage_verified OR NOT EXISTS (
        WITH latest AS (
            SELECT state.generation,state.store_path,state.timestamp,
                   state.generation_matches_current_store_path
            FROM systems system
            JOIN LATERAL (
                SELECT candidate.generation,candidate.store_path,candidate.timestamp,
                       candidate.generation_matches_current_store_path
                FROM system_states candidate
                WHERE candidate.hostname=system.hostname
                ORDER BY candidate.timestamp DESC NULLS LAST,candidate.id DESC
                LIMIT 1
            ) state ON true
            WHERE system.id=NEW.system_id AND system.is_active
        ), scoped AS (
            SELECT derivation.id,derivation.commit_id,derivation.derivation_name,
                   commit.source_archived
            FROM systems system
            JOIN derivations derivation ON derivation.derivation_type='nixos'
              AND derivation.derivation_name=COALESCE(
                  NULLIF(btrim(system.system_configuration_name),''),system.hostname)
            JOIN commits commit ON commit.id=derivation.commit_id
              AND commit.flake_id=system.flake_id
            JOIN latest ON COALESCE(derivation.store_path,derivation.expected_store_path)=latest.store_path
            WHERE system.id=NEW.system_id
        )
        SELECT 1 FROM latest
        JOIN scoped derivation ON derivation.id=NEW.derivation_id
          AND derivation.commit_id=NEW.commit_id
          AND derivation.derivation_name=NEW.configuration_name
        JOIN evaluation_snapshot_selections selection
          ON selection.commit_id=NEW.commit_id
         AND selection.configuration_name=NEW.configuration_name
         AND selection.current_snapshot_id=NEW.snapshot_id
        JOIN evaluation_snapshots artifact ON artifact.id=selection.current_snapshot_id
          AND artifact.lifecycle='available' AND artifact.schema_version=1
          AND artifact.integrity_version=1
          AND artifact.completed_at IS NOT NULL
          AND artifact.completed_at<=latest.timestamp
        WHERE latest.generation=NEW.generation
          AND latest.store_path=NEW.source_store_path
          AND latest.generation_matches_current_store_path IS TRUE
          AND latest.timestamp IS NOT NULL
          AND derivation.source_archived=false
          AND (SELECT COUNT(*) FROM scoped)=1
          AND EXISTS (
              SELECT 1 FROM cve_scans scan
              WHERE scan.derivation_id=NEW.derivation_id
                AND scan.status='completed'
                AND scan.completed_at IS NOT NULL
                AND scan.evidence_schema_version=1
          )
    ) THEN
        RAISE EXCEPTION 'external generation requires a unique current observation, certified selected artifact and exact schema-1 scan'
            USING ERRCODE='23514',
                  CONSTRAINT='external_generation_reconciliation_authority';
    END IF;
    RETURN NEW;
END;
$$;

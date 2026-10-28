-- Preserve the origin of a verified retained generation without modifying
-- immutable rows created before this migration. Pre-migration records include
-- both verified CF deployments and legacy unverified rows, so their origin is
-- deliberately unknown rather than inferred from a store path.
ALTER TABLE evaluation_generation_snapshots
    ADD COLUMN binding_origin text NOT NULL DEFAULT 'pre_reconciliation'
        CONSTRAINT evaluation_generation_binding_origin_check
        CHECK (binding_origin IN (
            'pre_reconciliation', 'cf_deployment', 'external_reconciled'
        ));

ALTER TABLE evaluation_generation_snapshots
    ALTER COLUMN binding_origin SET DEFAULT 'cf_deployment';

COMMENT ON COLUMN evaluation_generation_snapshots.binding_origin IS
    'Immutable retention provenance: pre_reconciliation is unknown historical origin; cf_deployment is an issued CF deployment; external_reconciled is a uniquely mapped observed activation. Origin does not change exact evidence requirements.';
COMMENT ON COLUMN evaluation_generation_snapshots.lineage_verified IS
    'True means exact observed generation, NixOS derivation, store path and certified evaluation artifact were verified. Deployment origin is recorded separately in binding_origin.';

-- The existing immutable-artifact trigger still validates every retained
-- insert and rejects updates. Only the external origin needs additional
-- observation, registry and scan checks at this trust boundary.
CREATE FUNCTION enforce_external_generation_reconciliation()
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
            SELECT derivation.id,derivation.commit_id,derivation.derivation_name
            FROM systems system
            JOIN derivations derivation ON derivation.derivation_type='nixos'
              AND derivation.derivation_name=COALESCE(
                  NULLIF(btrim(system.system_configuration_name),''),system.hostname)
            JOIN commits commit ON commit.id=derivation.commit_id
              AND commit.flake_id=system.flake_id
              AND commit.source_archived=false
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

CREATE TRIGGER evaluation_generation_external_reconciliation
BEFORE INSERT ON evaluation_generation_snapshots
FOR EACH ROW EXECUTE FUNCTION enforce_external_generation_reconciliation();

-- Retained observations preserve history after a mode change. Only managed
-- registrations can use those observations as Current CVE authority. Replace
-- in place to preserve dependent views, functions, and the public column shape.
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
    ) state ON true WHERE system.is_active AND system.management_mode='managed'
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

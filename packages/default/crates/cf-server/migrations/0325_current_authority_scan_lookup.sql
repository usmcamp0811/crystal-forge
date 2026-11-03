-- Restore indexed scan lookup in view_current_cve_authority.
--
-- Migration 0322 made the newest-scan lookup independent of candidate.derivation_id.
-- PostgreSQL then walked every completed scan newest-first and called
-- derivations_share_evidence_scope for each one, per managed system. At 1,000
-- managed systems and 180,000 scans a single-system read took 5.5 s and a fleet
-- read exceeded 120 s (0.12 ms and 15 ms before 0322). The Systems list, system
-- detail, environment rollups and the dashboard read this view.
--
-- INVARIANT: The result is identical to 0322. derivations_share_evidence_scope
-- requires COALESCE(reused_from_derivation_id, id) to be equal on both sides, so
-- every accepted candidate derivation is either the physical origin P or an
-- alias with reused_from_derivation_id = P. "peers" enumerates exactly that set
-- and is indexed by the primary key and derivations_reuse_origin_idx. The
-- original EXISTS predicate stays as the authoritative identity check on the
-- peer set; this migration adds no identity rule and removes none.
--
-- PERFORMANCE: "latest", "matching" and "unique_output" are NOT MATERIALIZED so
-- that a system_id filter reaches the per-system state lookup, as it did in 0314.
CREATE OR REPLACE VIEW view_current_cve_authority AS
WITH latest AS NOT MATERIALIZED (
    SELECT system.id AS system_id,system.environment_id,system.flake_id,
           COALESCE(NULLIF(btrim(system.system_configuration_name),''),system.hostname) AS configuration_name,
           state.generation,state.store_path,state.generation_matches_current_store_path
    FROM systems system
    JOIN LATERAL (
        SELECT candidate.generation,candidate.store_path,candidate.generation_matches_current_store_path
        FROM view_agent_current_system_states candidate WHERE candidate.hostname=system.hostname
        ORDER BY candidate.timestamp DESC NULLS LAST,candidate.id DESC LIMIT 1
    ) state ON true WHERE system.is_active AND system.management_mode='managed'
), matching AS NOT MATERIALIZED (
    SELECT observation.*,d.id AS derivation_id,
           COALESCE(d.reused_from_derivation_id,d.id) AS physical_derivation_id
    FROM latest observation
    JOIN commits c ON c.flake_id=observation.flake_id
    JOIN derivations d ON d.commit_id=c.id AND d.derivation_type='nixos'
      AND d.derivation_name=observation.configuration_name
      AND COALESCE(d.store_path,d.expected_store_path)=observation.store_path
    WHERE observation.generation IS NOT NULL
      AND btrim(COALESCE(observation.store_path,''))<>''
      AND observation.generation_matches_current_store_path IS TRUE
), unique_output AS NOT MATERIALIZED (
    SELECT system_id,environment_id,generation,store_path
    FROM matching GROUP BY system_id,environment_id,generation,store_path
    HAVING count(DISTINCT physical_derivation_id)=1
)
SELECT output.system_id,output.environment_id,output.generation,output.store_path,
       scan.derivation_id,source.commit_id,scan.id AS scan_id,scan.completed_at AS scan_completed_at,
       retained.id AS generation_snapshot_id,scan.scanner_name,scan.scanner_version
FROM unique_output output
JOIN LATERAL (
    SELECT candidate.* FROM (
        SELECT peer.id
        FROM matching target
        JOIN derivations peer ON peer.id=target.physical_derivation_id
        WHERE target.system_id=output.system_id
        UNION
        SELECT peer.id
        FROM matching target
        JOIN derivations peer ON peer.reused_from_derivation_id=target.physical_derivation_id
        WHERE target.system_id=output.system_id
    ) peers
    JOIN cve_scans candidate ON candidate.derivation_id=peers.id
    WHERE candidate.status='completed' AND candidate.completed_at IS NOT NULL
      AND candidate.evidence_schema_version=1
      AND EXISTS (SELECT 1 FROM matching target
                  WHERE target.system_id=output.system_id
                    AND (target.derivation_id=candidate.derivation_id
                         OR derivations_share_evidence_scope(target.derivation_id,candidate.derivation_id)))
    ORDER BY candidate.completed_at DESC,candidate.id DESC LIMIT 1
) scan ON true
JOIN derivations source ON source.id=scan.derivation_id
LEFT JOIN evaluation_generation_snapshots retained ON retained.system_id=output.system_id
  AND retained.generation=output.generation AND retained.source_store_path=output.store_path
  AND (retained.derivation_id=scan.derivation_id
       OR derivations_share_evidence_scope(retained.derivation_id,scan.derivation_id))
  AND retained.lineage_verified
  AND evaluation_deployment_artifact_matches(retained.snapshot_id,retained.derivation_id);

COMMENT ON VIEW view_current_cve_authority IS
    'Managed exact observed output, one enforced physical artifact, and actual immutable scan provenance. Supplemental retained configuration lineage may be a proven alias. Neither the selected head nor scan origin establishes unknown runtime Git revision.';

-- SECURITY: A scheduled deployment target is read authority only when the
-- latest live deployment request for a system has a complete immutable
-- commit, configuration, derivation, store-path, and evaluation-artifact
-- binding. Ranking precedes validation so an older valid request cannot mask
-- a newer incomplete request.
CREATE VIEW view_active_scheduled_cve_scan_targets AS
WITH ranked_active_deployments AS (
    SELECT deployment.*,
           row_number() OVER (
               PARTITION BY deployment.system_id
               ORDER BY deployment.issued_at DESC, deployment.id DESC
           ) AS active_rank
    FROM pending_system_deployments deployment
    WHERE deployment.status = 'pending'
      AND deployment.expires_at > now()
)
SELECT system.id AS system_id,
       system.environment_id,
       environment.name AS environment_name,
       deployment.id AS deployment_id,
       derivation.id AS derivation_id,
       commit.git_commit_hash AS commit_hash,
       scan.id AS scan_id,
       scan.completed_at
FROM ranked_active_deployments deployment
JOIN systems system
  ON system.id = deployment.system_id
 AND system.is_active
JOIN commits commit
  ON commit.id = deployment.requested_commit_id
 AND commit.flake_id = system.flake_id
JOIN derivations derivation
  ON derivation.id = deployment.requested_derivation_id
 AND derivation.commit_id = deployment.requested_commit_id
 AND derivation.derivation_type = 'nixos'
 AND derivation.derivation_name = COALESCE(
       NULLIF(btrim(system.system_configuration_name), ''), system.hostname
     )
 AND COALESCE(derivation.store_path, derivation.expected_store_path)
       = deployment.target_store_path
JOIN evaluation_snapshots artifact
  ON artifact.id = deployment.evaluation_snapshot_id
 AND artifact.commit_id = deployment.requested_commit_id
 AND artifact.configuration_name = derivation.derivation_name
 AND artifact.lifecycle = 'available'
 AND artifact.schema_version = 1
 AND artifact.integrity_version = 1
JOIN LATERAL (
    SELECT candidate.id, candidate.completed_at
    FROM cve_scans candidate
    WHERE candidate.derivation_id = derivation.id
      AND candidate.status = 'completed'
      AND candidate.completed_at IS NOT NULL
      AND candidate.evidence_schema_version = 1
    ORDER BY candidate.completed_at DESC, candidate.id DESC
    LIMIT 1
) scan ON true
LEFT JOIN environments environment ON environment.id = system.environment_id
WHERE deployment.active_rank = 1
  AND deployment.evaluation_snapshot_binding_expected;

CREATE VIEW view_active_scheduled_exact_cve_occurrences AS
SELECT target.system_id,
       target.environment_id,
       target.environment_name,
       target.deployment_id,
       target.derivation_id,
       target.commit_hash,
       target.scan_id,
       target.completed_at,
       observation.canonical_cve_id AS cve_id,
       observation.canonical_package_name AS package_name,
       observation.observed_package_version AS installed_version,
       observation.observed_derivation_path
FROM view_active_scheduled_cve_scan_targets target
JOIN cve_scan_vulnerability_observations observation
  ON observation.scan_id = target.scan_id
 AND NOT observation.is_whitelisted;

COMMENT ON VIEW view_active_scheduled_cve_scan_targets IS
  'Latest live deployment target per active system with exact commit, configuration, derivation, store-path, integrity-v1 evaluation artifact, and latest completed schema-1 CVE scan bindings.';
COMMENT ON VIEW view_active_scheduled_exact_cve_occurrences IS
  'Non-whitelisted exact CVE/package observations for actively scheduled deployment targets. This read-only inventory does not authorize triage or disposition coherence.';

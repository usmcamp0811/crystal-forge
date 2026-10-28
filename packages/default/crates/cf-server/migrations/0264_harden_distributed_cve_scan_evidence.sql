-- Adds scanner identity and evidence constraints without modifying the
-- previously released distributed lease migration.
ALTER TABLE builders
    ADD COLUMN cve_scanner_name text,
    ADD COLUMN cve_scanner_version text;

-- A builder must re-advertise its exact scanner identity after this upgrade.
UPDATE builders
SET cve_scanning_enabled = false,
    cve_scan_schema_version = 0
WHERE cve_scanning_enabled;

ALTER TABLE builders
    DROP CONSTRAINT builders_cve_scan_capability_coherent,
    ADD CONSTRAINT builders_cve_scan_capability_coherent CHECK (
        (NOT cve_scanning_enabled
            AND cve_scan_schema_version = 0
            AND cve_scanner_name IS NULL
            AND cve_scanner_version IS NULL)
        OR (cve_scanning_enabled
            AND cve_scan_schema_version = 1
            AND cve_scanner_name IS NOT NULL
            AND cve_scanner_name = 'vulnix'
            AND cve_scanner_version IS NOT NULL
            AND length(btrim(cve_scanner_version)) BETWEEN 1 AND 50)
    );

ALTER TABLE cve_scans
    ADD COLUMN closure_provenance text;

-- Earlier schema-1 results trusted the signed producing builder but did not
-- record whether the server could independently verify the target closure.
UPDATE cve_scans
SET closure_provenance = 'unverified_remote'
WHERE result_digest_sha256 IS NOT NULL;

ALTER TABLE cve_scans
    DROP CONSTRAINT cve_scans_remote_lease_coherent,
    ADD CONSTRAINT cve_scans_remote_lease_coherent CHECK (
        (lease_builder_id IS NULL
            AND lease_builder_session_id IS NULL
            AND lease_started_at IS NULL
            AND lease_heartbeat_at IS NULL
            AND lease_expires_at IS NULL)
        OR (execution_id IS NOT NULL
            AND lease_builder_id IS NOT NULL
            AND lease_builder_session_id IS NOT NULL
            AND lease_started_at IS NOT NULL
            AND lease_heartbeat_at IS NOT NULL
            AND lease_expires_at IS NOT NULL
            AND target_drv_path IS NOT NULL
            AND target_outputs IS NOT NULL
            AND scanner_policy IS NOT NULL
            AND status IN ('in_progress', 'completed'))
    ),
    ADD CONSTRAINT cve_scans_closure_provenance_valid CHECK (
        closure_provenance IS NULL
        OR closure_provenance IN ('server_local_verified', 'unverified_remote')
    ),
    ADD CONSTRAINT cve_scans_remote_result_coherent CHECK (
        result_digest_sha256 IS NULL
        OR (status = 'completed'
            AND execution_id IS NOT NULL
            AND execution_outcome IS NOT NULL
            AND execution_outcome = 'completed'
            AND closure_provenance IS NOT NULL
            AND evidence_schema_version IS NOT NULL
            AND evidence_schema_version = 1)
    ),
    ADD CONSTRAINT cve_scans_remote_outcome_coherent CHECK (
        execution_outcome IS NULL
        OR (execution_outcome = 'completed' AND status = 'completed')
        OR (execution_outcome = 'failed' AND status = 'failed')
        OR (execution_outcome = 'requeued' AND status = 'pending')
    );

ALTER TABLE cve_scan_vulnerability_observations
    ADD COLUMN is_affected boolean;

COMMENT ON COLUMN cve_scan_vulnerability_observations.is_affected IS
    'Whether the scanner reported the package as affected. This is independent from is_whitelisted; NULL means pre-0264 evidence whose affected marker cannot be reconstructed.';

COMMENT ON COLUMN cve_scans.closure_provenance IS
    'server_local_verified means the server verified closure membership and each package output deriver locally. unverified_remote means target closure data was unavailable on the server and the signed producing builder/session remains the explicit provenance boundary.';

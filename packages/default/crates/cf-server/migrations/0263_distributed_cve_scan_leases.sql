-- Server-owned lease state for database-free API builder CVE scanning.
-- Existing local executions continue to use scan_metadata until they are
-- claimed by the local fallback. New remote claims use these typed columns.
ALTER TABLE builders
    ADD COLUMN cve_scanning_enabled boolean NOT NULL DEFAULT false,
    ADD COLUMN cve_scan_schema_version integer NOT NULL DEFAULT 0,
    ADD CONSTRAINT builders_cve_scan_capability_coherent CHECK (
        (NOT cve_scanning_enabled AND cve_scan_schema_version = 0)
        OR (cve_scanning_enabled AND cve_scan_schema_version = 1)
    );

ALTER TABLE cve_scans
    ADD COLUMN source_trigger text NOT NULL DEFAULT 'legacy',
    ADD COLUMN completed_build_job_id uuid REFERENCES build_jobs(id) ON DELETE SET NULL,
    ADD COLUMN execution_id uuid,
    ADD COLUMN lease_builder_id uuid,
    ADD COLUMN lease_builder_session_id uuid,
    ADD COLUMN lease_started_at timestamptz,
    ADD COLUMN lease_heartbeat_at timestamptz,
    ADD COLUMN lease_expires_at timestamptz,
    ADD COLUMN scanner_policy jsonb,
    ADD COLUMN target_drv_path text,
    ADD COLUMN target_outputs jsonb,
    ADD COLUMN result_digest_sha256 varchar(64),
    ADD COLUMN execution_outcome text,
    ADD COLUMN failure_class text,
    ADD CONSTRAINT cve_scans_source_trigger_valid CHECK (
        source_trigger IN ('legacy', 'immediate', 'manual', 'fleet', 'post_build', 'periodic')
    ),
    ADD CONSTRAINT cve_scans_remote_lease_coherent CHECK (
        lease_builder_id IS NULL
        OR (execution_id IS NOT NULL
            AND lease_builder_id IS NOT NULL
            AND lease_builder_session_id IS NOT NULL
            AND lease_started_at IS NOT NULL
            AND lease_heartbeat_at IS NOT NULL
            AND lease_expires_at IS NOT NULL
            AND target_drv_path IS NOT NULL
            AND target_outputs IS NOT NULL
            AND scanner_policy IS NOT NULL)
    ),
    ADD CONSTRAINT cve_scans_result_digest_valid CHECK (
        result_digest_sha256 IS NULL
        OR result_digest_sha256 ~ '^[0-9a-f]{64}$'
    ),
    ADD CONSTRAINT cve_scans_execution_outcome_valid CHECK (
        execution_outcome IS NULL OR execution_outcome IN ('completed', 'failed', 'requeued')
    ),
    ADD CONSTRAINT cve_scans_failure_class_valid CHECK (
        failure_class IS NULL
        OR failure_class IN ('transient', 'deterministic', 'authorization', 'cancelled')
    );

CREATE UNIQUE INDEX cve_scans_one_remote_lease_per_builder
    ON cve_scans(lease_builder_id)
    WHERE status = 'in_progress' AND lease_builder_id IS NOT NULL;

CREATE INDEX cve_scans_remote_claim_queue
    ON cve_scans(source_trigger, created_at, id)
    WHERE status = 'pending';

COMMENT ON COLUMN cve_scans.execution_id IS
    'Typed fencing token for remote scan executions. Local legacy execution tokens remain in scan_metadata.';
COMMENT ON COLUMN cve_scans.lease_builder_session_id IS
    'Builder process session that owns the remote execution. Session replacement fences all later writes.';
COMMENT ON COLUMN cve_scans.lease_builder_id IS
    'Builder UUID that owns or completed the remote execution. It intentionally has no foreign key so builder deletion cannot mutate immutable schema-1 evidence.';
COMMENT ON COLUMN cve_scans.result_digest_sha256 IS
    'Server-recomputed SHA-256 of canonical schema-1 evidence. Equal retries are idempotent; unequal retries conflict.';
COMMENT ON COLUMN cve_scans.target_outputs IS
    'Exact server-authorized output-name-to-store-path mapping sealed when the lease is claimed.';

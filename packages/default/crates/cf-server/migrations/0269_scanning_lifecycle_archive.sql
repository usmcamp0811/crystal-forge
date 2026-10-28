-- Adds durable scan wait states and mutable archive presentation metadata.
-- Scan evidence remains in cve_scans and keeps its existing immutability seal.

ALTER TABLE cve_scans
    DROP CONSTRAINT cve_scans_source_trigger_valid,
    ALTER COLUMN source_trigger DROP NOT NULL;

DROP INDEX idx_cve_scans_unique_active;
CREATE UNIQUE INDEX idx_cve_scans_unique_active
    ON cve_scans (derivation_id)
    WHERE status IN (
        'awaiting_build',
        'awaiting_closure',
        'pending',
        'in_progress'
    );

CREATE INDEX cve_scans_waiting_prerequisites
    ON cve_scans (status, created_at, id)
    WHERE status IN ('awaiting_build', 'awaiting_closure');

CREATE TABLE cve_scan_archives (
    scan_id uuid PRIMARY KEY REFERENCES cve_scans(id) ON DELETE CASCADE,
    archived_at timestamptz NOT NULL DEFAULT NOW(),
    archived_by uuid NOT NULL REFERENCES users(id) ON DELETE RESTRICT
);

CREATE INDEX cve_scan_archives_archived_at
    ON cve_scan_archives (archived_at DESC, scan_id DESC);

COMMENT ON COLUMN cve_scans.status IS
    'Lifecycle state. awaiting_build and awaiting_closure are active but unowned; pending is runnable; in_progress is execution-owned; completed and failed are terminal.';
COMMENT ON COLUMN cve_scans.source_trigger IS
    'Immutable raw trigger provenance. NULL and legacy identify old rows; unknown values are retained for forward-compatible presentation.';
COMMENT ON TABLE cve_scan_archives IS
    'Mutable administrator presentation state for terminal scans. Archive changes never mutate cve_scans evidence or diagnostics.';

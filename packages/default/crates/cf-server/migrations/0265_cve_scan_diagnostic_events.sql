-- Bounded, append-only diagnostics for individual CVE scan executions.
-- Vulnerability evidence remains authoritative in the existing schema-1 tables.
CREATE TABLE cve_scan_diagnostic_events (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    scan_id uuid NOT NULL REFERENCES cve_scans(id) ON DELETE CASCADE,
    execution_id uuid NOT NULL,
    attempt_number integer NOT NULL CHECK (attempt_number > 0),
    occurred_at timestamptz NOT NULL,
    level text NOT NULL CHECK (level IN ('info', 'warning', 'error')),
    source text NOT NULL CHECK (source IN ('server', 'builder', 'vulnix', 'nix')),
    event_type text NOT NULL CHECK (
        event_type IN ('attempt_started', 'output', 'attempt_completed', 'attempt_failed', 'attempt_requeued')
    ),
    message text NOT NULL CHECK (char_length(message) BETWEEN 1 AND 2048),
    truncated boolean NOT NULL DEFAULT false,
    created_at timestamptz NOT NULL DEFAULT NOW()
);

CREATE INDEX cve_scan_diagnostic_events_scan_order
    ON cve_scan_diagnostic_events(scan_id, occurred_at, id);

CREATE UNIQUE INDEX cve_scan_diagnostic_events_attempt_lifecycle
    ON cve_scan_diagnostic_events(scan_id, execution_id, event_type)
    WHERE event_type IN ('attempt_started', 'attempt_completed', 'attempt_failed', 'attempt_requeued');

CREATE FUNCTION prevent_cve_scan_diagnostic_event_mutation()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    -- A parent scan deletion can remove its diagnostics through the foreign-key
    -- cascade. Direct event mutation is forbidden after the append transaction.
    IF TG_OP = 'UPDATE' OR pg_trigger_depth() = 1 THEN
        RAISE EXCEPTION 'CVE scan diagnostic events are immutable';
    END IF;
    RETURN OLD;
END;
$$;

CREATE TRIGGER trigger_prevent_cve_scan_diagnostic_event_mutation
    BEFORE UPDATE OR DELETE ON cve_scan_diagnostic_events
    FOR EACH ROW EXECUTE FUNCTION prevent_cve_scan_diagnostic_event_mutation();

COMMENT ON TABLE cve_scan_diagnostic_events IS
    'Append-only redacted diagnostic events for one fenced CVE scan execution. These rows are operational detail and are not vulnerability evidence.';
COMMENT ON COLUMN cve_scan_diagnostic_events.execution_id IS
    'Immutable local or remote execution token. Every append is fenced against the owning cve_scans execution before insertion.';

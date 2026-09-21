-- Add idempotent execution-phase events for live remote scan diagnostics.

ALTER TABLE cve_scan_diagnostic_events
    DROP CONSTRAINT cve_scan_diagnostic_events_event_type_check;

ALTER TABLE cve_scan_diagnostic_events
    ADD CONSTRAINT cve_scan_diagnostic_events_event_type_check CHECK (
        event_type IN (
            'attempt_started',
            'materialization_started',
            'materialization_completed',
            'scanner_started',
            'scanner_completed',
            'evidence_resolution_started',
            'evidence_resolution_completed',
            'output',
            'attempt_completed',
            'attempt_failed',
            'result_persistence_failed',
            'attempt_requeued'
        )
    );

DROP INDEX cve_scan_diagnostic_events_attempt_lifecycle;

CREATE UNIQUE INDEX cve_scan_diagnostic_events_attempt_lifecycle
    ON cve_scan_diagnostic_events(scan_id, execution_id, event_type)
    WHERE event_type IN (
        'attempt_started',
        'materialization_started',
        'materialization_completed',
        'scanner_started',
        'scanner_completed',
        'evidence_resolution_started',
        'evidence_resolution_completed',
        'attempt_completed',
        'attempt_failed',
        'result_persistence_failed',
        'attempt_requeued'
    );

DROP INDEX cve_scan_diagnostic_events_scan_order;

CREATE INDEX cve_scan_diagnostic_events_received_order
    ON cve_scan_diagnostic_events(scan_id, attempt_number, id);

COMMENT ON COLUMN cve_scan_diagnostic_events.event_type IS
    'Bounded lifecycle, execution-phase, or scanner output event kind.';

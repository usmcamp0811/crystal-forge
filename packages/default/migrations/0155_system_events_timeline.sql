CREATE TABLE IF NOT EXISTS pending_system_deployments (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    system_id uuid NOT NULL REFERENCES systems (id) ON DELETE CASCADE,
    target_store_path text NOT NULL,
    status text NOT NULL DEFAULT 'pending',
    source text NOT NULL DEFAULT 'desired_target',
    issued_at timestamptz NOT NULL DEFAULT now(),
    expires_at timestamptz NOT NULL DEFAULT now() + interval '2 hours',
    completed_at timestamptz,
    superseded_by uuid REFERENCES pending_system_deployments (id) ON DELETE SET NULL,
    metadata jsonb NOT NULL DEFAULT '{}'::jsonb,
    CONSTRAINT pending_system_deployments_status_check CHECK (
        status IN ('pending', 'succeeded', 'failed', 'superseded', 'expired')
    )
);

CREATE INDEX IF NOT EXISTS idx_pending_system_deployments_system_status
    ON pending_system_deployments (system_id, status, issued_at DESC);

CREATE INDEX IF NOT EXISTS idx_pending_system_deployments_target_pending
    ON pending_system_deployments (system_id, target_store_path, issued_at DESC)
    WHERE status = 'pending';

CREATE TABLE IF NOT EXISTS system_events (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    system_id uuid NOT NULL REFERENCES systems (id) ON DELETE CASCADE,
    event_type text NOT NULL,
    dedupe_key text NOT NULL,
    correlation_id uuid,
    occurred_at timestamptz NOT NULL,
    observed_at timestamptz NOT NULL DEFAULT now(),
    previous_generation bigint,
    new_generation bigint,
    previous_store_path text,
    new_store_path text,
    previous_boot_id text,
    new_boot_id text,
    deployment_id uuid REFERENCES pending_system_deployments (id) ON DELETE SET NULL,
    desired_target_id uuid REFERENCES pending_system_deployments (id) ON DELETE SET NULL,
    source text NOT NULL,
    actor text,
    metadata jsonb NOT NULL DEFAULT '{}'::jsonb,
    CONSTRAINT system_events_event_type_check CHECK (
        event_type IN (
            'system_reboot',
            'agent_restart',
            'cf_deployment_started',
            'cf_deployment_succeeded',
            'cf_deployment_failed',
            'local_rebuild_detected'
        )
    ),
    CONSTRAINT system_events_dedupe_unique UNIQUE (system_id, event_type, dedupe_key)
);

CREATE INDEX IF NOT EXISTS idx_system_events_system_history
    ON system_events (system_id, occurred_at DESC, observed_at DESC, id DESC);

CREATE INDEX IF NOT EXISTS idx_system_events_system_type
    ON system_events (system_id, event_type, occurred_at DESC);
